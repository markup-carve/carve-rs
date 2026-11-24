//! Compact JSON with object keys in byte order: the text
//! `serde_json::to_value(v)?.to_string()` gives, written without building a
//! `Value` (carve-rs#4619).
//!
//! Each object keeps its entries as pieces of one buffer and sorts them when
//! it closes; the last pass copies the pieces in order, so every byte moves a
//! fixed number of times at any nesting depth. Shapes a `Value` would accept
//! but this writer does not (a non-string map key, a 128-bit integer) are an
//! error, and the caller falls back to the `Value` path.

use serde::ser::{self, Serialize};

#[derive(Debug)]
pub(crate) struct Error;

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("value outside the canonical JSON writer")
    }
}

impl std::error::Error for Error {}

impl ser::Error for Error {
    fn custom<T: std::fmt::Display>(_: T) -> Self {
        Self
    }
}

/// `v` as canonical JSON text.
pub(crate) fn to_string<T: Serialize + ?Sized>(v: &T) -> Result<String, Error> {
    let mut enc = Encoder::default();
    let root = v.serialize(&mut enc)?;
    let mut out = Vec::with_capacity(enc.buf.len() + enc.done.len() * 2 + 16);
    enc.emit(root, &mut out);
    String::from_utf8(out).map_err(|_| Error)
}

#[derive(Clone, Copy)]
enum Piece {
    /// Finished JSON text in `buf`.
    Leaf(u32, u32),
    /// Items in `done`.
    Array(u32, u32),
    /// Entries in `done`, sorted.
    Object(u32, u32),
}

#[derive(Clone, Copy)]
struct Entry {
    /// The raw key in `buf`; unused for an array item.
    key: (u32, u32),
    piece: Piece,
}

#[derive(Default)]
struct Encoder {
    buf: Vec<u8>,
    done: Vec<Entry>,
    /// Entries of the composites still open, innermost last.
    open: Vec<Entry>,
}

fn at(n: usize) -> Result<u32, Error> {
    u32::try_from(n).map_err(|_| Error)
}

impl Encoder {
    fn leaf<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<Piece, Error> {
        let start = at(self.buf.len())?;
        serde_json::to_writer(&mut self.buf, v).map_err(|_| Error)?;
        Ok(Piece::Leaf(start, at(self.buf.len())?))
    }

    fn raw_key(&mut self, key: &str) -> Result<(u32, u32), Error> {
        let start = at(self.buf.len())?;
        self.buf.extend_from_slice(key.as_bytes());
        Ok((start, at(self.buf.len())?))
    }

    fn key_bytes(&self, key: (u32, u32)) -> &[u8] {
        self.buf
            .get(key.0 as usize..key.1 as usize)
            .unwrap_or_default()
    }

    fn close_array(&mut self, mark: usize) -> Result<Piece, Error> {
        let start = at(self.done.len())?;
        self.done.extend(self.open.drain(mark..));
        Ok(Piece::Array(start, at(self.done.len())?))
    }

    fn close_object(&mut self, mark: usize) -> Result<Piece, Error> {
        let buf = &self.buf;
        let entries = self.open.get_mut(mark..).ok_or(Error)?;
        // The open range belongs to this object, so sorting it needs no
        // intermediate collection for each AST node.
        entries.sort_by(|a, b| {
            buf.get(a.key.0 as usize..a.key.1 as usize)
                .unwrap_or_default()
                .cmp(
                    buf.get(b.key.0 as usize..b.key.1 as usize)
                        .unwrap_or_default(),
                )
        });
        let start = at(self.done.len())?;
        for (i, e) in entries.iter().enumerate() {
            // A map keeps the last value of a repeated key.
            let repeated = entries.get(i + 1).is_some_and(|next| {
                buf.get(next.key.0 as usize..next.key.1 as usize)
                    .unwrap_or_default()
                    == buf
                        .get(e.key.0 as usize..e.key.1 as usize)
                        .unwrap_or_default()
            });
            if !repeated {
                self.done.push(*e);
            }
        }
        self.open.truncate(mark);
        Ok(Piece::Object(start, at(self.done.len())?))
    }

    fn emit(&self, root: Piece, out: &mut Vec<u8>) {
        enum Step {
            Piece(Piece),
            /// A comma unless first, then the escaped key and a colon.
            Key((u32, u32), bool),
            Byte(u8),
        }
        let mut todo = vec![Step::Piece(root)];
        while let Some(step) = todo.pop() {
            match step {
                Step::Byte(b) => out.push(b),
                Step::Key(key, first) => {
                    if !first {
                        out.push(b',');
                    }
                    let key = std::str::from_utf8(self.key_bytes(key)).unwrap_or_default();
                    let _ = serde_json::to_writer(&mut *out, key);
                    out.push(b':');
                }
                Step::Piece(Piece::Leaf(s, e)) => {
                    out.extend_from_slice(self.buf.get(s as usize..e as usize).unwrap_or_default());
                }
                Step::Piece(Piece::Array(s, e)) => {
                    out.push(b'[');
                    todo.push(Step::Byte(b']'));
                    let items = self.done.get(s as usize..e as usize).unwrap_or_default();
                    for (i, item) in items.iter().enumerate().rev() {
                        todo.push(Step::Piece(item.piece));
                        if i > 0 {
                            todo.push(Step::Byte(b','));
                        }
                    }
                }
                Step::Piece(Piece::Object(s, e)) => {
                    out.push(b'{');
                    todo.push(Step::Byte(b'}'));
                    let entries = self.done.get(s as usize..e as usize).unwrap_or_default();
                    for (i, entry) in entries.iter().enumerate().rev() {
                        todo.push(Step::Piece(entry.piece));
                        todo.push(Step::Key(entry.key, i == 0));
                    }
                }
            }
        }
    }
}

/// An open array or object: its entries start at `mark` in `Encoder::open`.
struct Compound<'e> {
    enc: &'e mut Encoder,
    mark: usize,
    key: (u32, u32),
    /// For a variant wrapper: the variant name the closed body goes under.
    variant: Option<&'static str>,
}

impl Compound<'_> {
    fn item<T: Serialize + ?Sized>(&mut self, key: (u32, u32), v: &T) -> Result<(), Error> {
        let piece = v.serialize(&mut *self.enc)?;
        self.enc.open.push(Entry { key, piece });
        Ok(())
    }

    fn field<T: Serialize + ?Sized>(&mut self, key: &str, v: &T) -> Result<(), Error> {
        let key = self.enc.raw_key(key)?;
        self.item(key, v)
    }

    fn wrap(self, body: Piece) -> Result<Piece, Error> {
        let Some(variant) = self.variant else {
            return Ok(body);
        };
        let key = self.enc.raw_key(variant)?;
        let mark = self.enc.open.len();
        self.enc.open.push(Entry { key, piece: body });
        self.enc.close_object(mark)
    }

    fn end_array(self) -> Result<Piece, Error> {
        let body = self.enc.close_array(self.mark)?;
        self.wrap(body)
    }

    fn end_object(self) -> Result<Piece, Error> {
        let body = self.enc.close_object(self.mark)?;
        self.wrap(body)
    }
}

impl<'e> ser::Serializer for &'e mut Encoder {
    type Ok = Piece;
    type Error = Error;
    type SerializeSeq = Compound<'e>;
    type SerializeTuple = Compound<'e>;
    type SerializeTupleStruct = Compound<'e>;
    type SerializeTupleVariant = Compound<'e>;
    type SerializeMap = Compound<'e>;
    type SerializeStruct = Compound<'e>;
    type SerializeStructVariant = Compound<'e>;

    fn serialize_bool(self, v: bool) -> Result<Piece, Error> {
        self.leaf(&v)
    }
    fn serialize_i8(self, v: i8) -> Result<Piece, Error> {
        self.leaf(&v)
    }
    fn serialize_i16(self, v: i16) -> Result<Piece, Error> {
        self.leaf(&v)
    }
    fn serialize_i32(self, v: i32) -> Result<Piece, Error> {
        self.leaf(&v)
    }
    fn serialize_i64(self, v: i64) -> Result<Piece, Error> {
        self.leaf(&v)
    }
    fn serialize_u8(self, v: u8) -> Result<Piece, Error> {
        self.leaf(&v)
    }
    fn serialize_u16(self, v: u16) -> Result<Piece, Error> {
        self.leaf(&v)
    }
    fn serialize_u32(self, v: u32) -> Result<Piece, Error> {
        self.leaf(&v)
    }
    fn serialize_u64(self, v: u64) -> Result<Piece, Error> {
        self.leaf(&v)
    }
    fn serialize_i128(self, _: i128) -> Result<Piece, Error> {
        Err(Error)
    }
    fn serialize_u128(self, _: u128) -> Result<Piece, Error> {
        Err(Error)
    }
    fn serialize_f32(self, v: f32) -> Result<Piece, Error> {
        // A `Value` holds every float as f64.
        self.leaf(&f64::from(v))
    }
    fn serialize_f64(self, v: f64) -> Result<Piece, Error> {
        self.leaf(&v)
    }
    fn serialize_char(self, v: char) -> Result<Piece, Error> {
        self.leaf(&v)
    }
    fn serialize_str(self, v: &str) -> Result<Piece, Error> {
        self.leaf(v)
    }
    fn serialize_bytes(self, v: &[u8]) -> Result<Piece, Error> {
        let mark = self.open.len();
        for b in v {
            let piece = self.leaf(b)?;
            self.open.push(Entry { key: (0, 0), piece });
        }
        self.close_array(mark)
    }
    fn serialize_none(self) -> Result<Piece, Error> {
        self.leaf(&())
    }
    fn serialize_some<T: Serialize + ?Sized>(self, v: &T) -> Result<Piece, Error> {
        v.serialize(self)
    }
    fn serialize_unit(self) -> Result<Piece, Error> {
        self.leaf(&())
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<Piece, Error> {
        self.leaf(&())
    }
    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
    ) -> Result<Piece, Error> {
        self.leaf(variant)
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        name: &'static str,
        v: &T,
    ) -> Result<Piece, Error> {
        if name.starts_with("$serde_json::") {
            return Err(Error);
        }
        v.serialize(self)
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        v: &T,
    ) -> Result<Piece, Error> {
        let mark = self.open.len();
        let mut c = Compound {
            enc: self,
            mark,
            key: (0, 0),
            variant: None,
        };
        c.field(variant, v)?;
        c.end_object()
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Compound<'e>, Error> {
        Ok(self.compound(None))
    }
    fn serialize_tuple(self, _: usize) -> Result<Compound<'e>, Error> {
        Ok(self.compound(None))
    }
    fn serialize_tuple_struct(self, _: &'static str, _: usize) -> Result<Compound<'e>, Error> {
        Ok(self.compound(None))
    }
    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        _: usize,
    ) -> Result<Compound<'e>, Error> {
        Ok(self.compound(Some(variant)))
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Compound<'e>, Error> {
        Ok(self.compound(None))
    }
    fn serialize_struct(self, name: &'static str, _: usize) -> Result<Compound<'e>, Error> {
        if name.starts_with("$serde_json::") {
            return Err(Error);
        }
        Ok(self.compound(None))
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        _: usize,
    ) -> Result<Compound<'e>, Error> {
        Ok(self.compound(Some(variant)))
    }
}

impl<'e> Encoder {
    fn compound(&'e mut self, variant: Option<&'static str>) -> Compound<'e> {
        let mark = self.open.len();
        Compound {
            enc: self,
            mark,
            key: (0, 0),
            variant,
        }
    }
}

impl ser::SerializeSeq for Compound<'_> {
    type Ok = Piece;
    type Error = Error;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Error> {
        self.item((0, 0), v)
    }
    fn end(self) -> Result<Piece, Error> {
        self.end_array()
    }
}

impl ser::SerializeTuple for Compound<'_> {
    type Ok = Piece;
    type Error = Error;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Error> {
        self.item((0, 0), v)
    }
    fn end(self) -> Result<Piece, Error> {
        self.end_array()
    }
}

impl ser::SerializeTupleStruct for Compound<'_> {
    type Ok = Piece;
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Error> {
        self.item((0, 0), v)
    }
    fn end(self) -> Result<Piece, Error> {
        self.end_array()
    }
}

impl ser::SerializeTupleVariant for Compound<'_> {
    type Ok = Piece;
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Error> {
        self.item((0, 0), v)
    }
    fn end(self) -> Result<Piece, Error> {
        self.end_array()
    }
}

impl ser::SerializeMap for Compound<'_> {
    type Ok = Piece;
    type Error = Error;
    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<(), Error> {
        self.key = key.serialize(KeyCapture(&mut *self.enc))?;
        Ok(())
    }
    fn serialize_value<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Error> {
        self.item(self.key, v)
    }
    fn end(self) -> Result<Piece, Error> {
        self.end_object()
    }
}

impl ser::SerializeStruct for Compound<'_> {
    type Ok = Piece;
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        v: &T,
    ) -> Result<(), Error> {
        self.field(key, v)
    }
    fn end(self) -> Result<Piece, Error> {
        self.end_object()
    }
}

impl ser::SerializeStructVariant for Compound<'_> {
    type Ok = Piece;
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        v: &T,
    ) -> Result<(), Error> {
        self.field(key, v)
    }
    fn end(self) -> Result<Piece, Error> {
        self.end_object()
    }
}

/// Takes a map key as raw text; only string keys are covered.
struct KeyCapture<'e>(&'e mut Encoder);

impl ser::Serializer for KeyCapture<'_> {
    type Ok = (u32, u32);
    type Error = Error;
    type SerializeSeq = ser::Impossible<(u32, u32), Error>;
    type SerializeTuple = ser::Impossible<(u32, u32), Error>;
    type SerializeTupleStruct = ser::Impossible<(u32, u32), Error>;
    type SerializeTupleVariant = ser::Impossible<(u32, u32), Error>;
    type SerializeMap = ser::Impossible<(u32, u32), Error>;
    type SerializeStruct = ser::Impossible<(u32, u32), Error>;
    type SerializeStructVariant = ser::Impossible<(u32, u32), Error>;

    fn serialize_str(self, v: &str) -> Result<(u32, u32), Error> {
        self.0.raw_key(v)
    }
    fn serialize_char(self, v: char) -> Result<(u32, u32), Error> {
        self.0.raw_key(v.encode_utf8(&mut [0; 4]))
    }
    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
    ) -> Result<(u32, u32), Error> {
        self.0.raw_key(variant)
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        v: &T,
    ) -> Result<(u32, u32), Error> {
        v.serialize(self)
    }
    fn serialize_bool(self, _: bool) -> Result<(u32, u32), Error> {
        Err(Error)
    }
    fn serialize_i8(self, _: i8) -> Result<(u32, u32), Error> {
        Err(Error)
    }
    fn serialize_i16(self, _: i16) -> Result<(u32, u32), Error> {
        Err(Error)
    }
    fn serialize_i32(self, _: i32) -> Result<(u32, u32), Error> {
        Err(Error)
    }
    fn serialize_i64(self, _: i64) -> Result<(u32, u32), Error> {
        Err(Error)
    }
    fn serialize_u8(self, _: u8) -> Result<(u32, u32), Error> {
        Err(Error)
    }
    fn serialize_u16(self, _: u16) -> Result<(u32, u32), Error> {
        Err(Error)
    }
    fn serialize_u32(self, _: u32) -> Result<(u32, u32), Error> {
        Err(Error)
    }
    fn serialize_u64(self, _: u64) -> Result<(u32, u32), Error> {
        Err(Error)
    }
    fn serialize_f32(self, _: f32) -> Result<(u32, u32), Error> {
        Err(Error)
    }
    fn serialize_f64(self, _: f64) -> Result<(u32, u32), Error> {
        Err(Error)
    }
    fn serialize_bytes(self, _: &[u8]) -> Result<(u32, u32), Error> {
        Err(Error)
    }
    fn serialize_none(self) -> Result<(u32, u32), Error> {
        Err(Error)
    }
    fn serialize_some<T: Serialize + ?Sized>(self, _: &T) -> Result<(u32, u32), Error> {
        Err(Error)
    }
    fn serialize_unit(self) -> Result<(u32, u32), Error> {
        Err(Error)
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<(u32, u32), Error> {
        Err(Error)
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: &T,
    ) -> Result<(u32, u32), Error> {
        Err(Error)
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Self::SerializeSeq, Error> {
        Err(Error)
    }
    fn serialize_tuple(self, _: usize) -> Result<Self::SerializeTuple, Error> {
        Err(Error)
    }
    fn serialize_tuple_struct(
        self,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleStruct, Error> {
        Err(Error)
    }
    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleVariant, Error> {
        Err(Error)
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Self::SerializeMap, Error> {
        Err(Error)
    }
    fn serialize_struct(self, _: &'static str, _: usize) -> Result<Self::SerializeStruct, Error> {
        Err(Error)
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStructVariant, Error> {
        Err(Error)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use serde::Serialize;
    use serde_json::json;

    fn same<T: Serialize>(v: &T) {
        let expected = serde_json::to_value(v).unwrap().to_string();
        assert_eq!(super::to_string(v).unwrap(), expected);
    }

    #[test]
    fn matches_the_value_encoding() {
        same(
            &json!({"z": 1, "a": [1.5, -2, null, true], "m": {"b": "\"q\"\n", "\u{e9}": 0, "A": {}}}),
        );
        same(&json!([[], {}, "", 1e300, 0.1, u64::MAX, i64::MIN]));
        same(&f32::from_bits(0x3dcc_cccd));
        same(&f64::NAN);
        same(&b"by".as_slice());
        same(&std::collections::HashMap::from([
            ("k\"ey", 1),
            ("a\\b", 2),
            ("\u{7f}", 3),
        ]));
    }

    #[test]
    fn enums_and_flatten_match_the_value_encoding() {
        #[derive(Serialize)]
        #[serde(tag = "type")]
        enum Tagged {
            Leaf { value: u8 },
        }
        #[derive(Serialize)]
        enum Plain {
            Unit,
            New(u8),
            Tuple(u8, u8),
            Struct { b: u8, a: u8 },
        }
        #[derive(Serialize)]
        struct Outer {
            z: u8,
            #[serde(flatten)]
            rest: std::collections::BTreeMap<String, u8>,
        }
        same(&Tagged::Leaf { value: 3 });
        same(&vec![
            Plain::Unit,
            Plain::New(1),
            Plain::Tuple(1, 2),
            Plain::Struct { b: 1, a: 2 },
        ]);
        // A flattened key repeats a field: the last value wins, as in a map.
        same(&Outer {
            z: 1,
            rest: [("z".to_string(), 9), ("a".to_string(), 2)].into(),
        });
    }

    #[test]
    fn deep_nesting_matches_the_value_encoding() {
        let mut v = json!(1);
        for i in 0..2000 {
            v = json!({"children": [v], "type": i});
        }
        same(&v);
    }

    #[test]
    fn non_string_keys_fall_back() {
        assert!(super::to_string(&std::collections::BTreeMap::from([(1, 2)])).is_err());
    }
}
