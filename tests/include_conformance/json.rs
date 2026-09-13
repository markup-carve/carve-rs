//! Minimal std-only JSON reader for the include-conformance vectors.
//!
//! carve-rs ships ZERO runtime dependencies and does not commit a `Cargo.lock`,
//! and CI pins a Rust 1.75 MSRV leg. A floating `serde_json` dev-dependency
//! would therefore risk breaking that leg the day a future `serde_json` patch
//! raised its own MSRV above 1.75 (cargo 1.75 has no MSRV-aware resolution and
//! there is no lockfile to pin it). The vectors are well-formed generator
//! output, so a small hand-rolled reader is both sufficient and keeps the test
//! suite dependency-free like the rest of the crate.
//!
//! This is a reader, not a validator: it accepts the JSON the suite emits. It
//! is exercised against all 90 vectors on every run, and the runner's tamper
//! test confirms it round-trips their content faithfully.

use std::ops::Index;

/// A parsed JSON value. Objects keep insertion order, which the runner relies
/// on for first-encounter dependency order and tree-materialization order.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Value>),
    Obj(Vec<(String, Value)>),
}

impl Value {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_u64(&self) -> Option<u64> {
        match self {
            // The vectors only carry non-negative integer knobs (maxDepth,
            // maxBytes); reject a non-integral or negative number rather than
            // silently truncating.
            Value::Num(n) if *n >= 0.0 && n.fract() == 0.0 => Some(*n as u64),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&Vec<Value>> {
        match self {
            Value::Arr(a) => Some(a),
            _ => None,
        }
    }

    /// Object member by key, or `None` for a missing key or a non-object.
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Obj(pairs) => pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// Object members in insertion order, or an empty slice for a non-object.
    pub fn entries(&self) -> &[(String, Value)] {
        match self {
            Value::Obj(pairs) => pairs,
            _ => &[],
        }
    }

    pub fn is_object(&self) -> bool {
        matches!(self, Value::Obj(_))
    }

    pub fn parse(input: &str) -> Result<Value, String> {
        let mut p = Parser {
            bytes: input.as_bytes(),
            pos: 0,
        };
        p.skip_ws();
        let v = p.value()?;
        p.skip_ws();
        if p.pos != p.bytes.len() {
            return Err(format!("trailing input at byte {}", p.pos));
        }
        Ok(v)
    }
}

/// Indexing by `&str` mirrors serde_json: a missing key or non-object yields a
/// shared `Null`, so `v["missing"].as_str()` is `None` rather than a panic.
impl Index<&str> for Value {
    type Output = Value;
    fn index(&self, key: &str) -> &Value {
        static NULL: Value = Value::Null;
        self.get(key).unwrap_or(&NULL)
    }
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Parser<'_> {
    fn skip_ws(&mut self) {
        while let Some(&b) = self.bytes.get(self.pos) {
            if matches!(b, b' ' | b'\t' | b'\n' | b'\r') {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn value(&mut self) -> Result<Value, String> {
        match self.bytes.get(self.pos) {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => Ok(Value::Str(self.string()?)),
            Some(b't') => self.literal("true", Value::Bool(true)),
            Some(b'f') => self.literal("false", Value::Bool(false)),
            Some(b'n') => self.literal("null", Value::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            other => Err(format!("unexpected token {other:?} at byte {}", self.pos)),
        }
    }

    fn literal(&mut self, word: &str, val: Value) -> Result<Value, String> {
        if self.bytes[self.pos..].starts_with(word.as_bytes()) {
            self.pos += word.len();
            Ok(val)
        } else {
            Err(format!("invalid literal at byte {}", self.pos))
        }
    }

    fn number(&mut self) -> Result<Value, String> {
        let start = self.pos;
        while let Some(&b) = self.bytes.get(self.pos) {
            if matches!(b, b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9') {
                self.pos += 1;
            } else {
                break;
            }
        }
        let text = std::str::from_utf8(&self.bytes[start..self.pos]).map_err(|e| e.to_string())?;
        text.parse::<f64>()
            .map(Value::Num)
            .map_err(|e| format!("bad number {text:?}: {e}"))
    }

    fn string(&mut self) -> Result<String, String> {
        // Assumes the opening quote is at self.pos.
        self.pos += 1;
        let mut out = String::new();
        loop {
            let b = *self
                .bytes
                .get(self.pos)
                .ok_or_else(|| "unterminated string".to_string())?;
            self.pos += 1;
            match b {
                b'"' => return Ok(out),
                b'\\' => {
                    let esc = *self
                        .bytes
                        .get(self.pos)
                        .ok_or_else(|| "unterminated escape".to_string())?;
                    self.pos += 1;
                    match esc {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{0008}'),
                        b'f' => out.push('\u{000C}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => out.push(self.unicode_escape()?),
                        other => return Err(format!("bad escape \\{}", other as char)),
                    }
                }
                // A raw UTF-8 byte: copy the whole code point through. The
                // vectors carry literal UTF-8 (curly quotes etc.), so this is
                // the common path.
                _ => {
                    let len = utf8_len(b);
                    let ch_start = self.pos - 1;
                    self.pos = ch_start + len;
                    let slice = self
                        .bytes
                        .get(ch_start..self.pos)
                        .ok_or_else(|| "truncated utf-8 in string".to_string())?;
                    out.push_str(std::str::from_utf8(slice).map_err(|e| e.to_string())?);
                }
            }
        }
    }

    /// Read a `\uXXXX` escape (the `\u` already consumed), pairing a leading
    /// surrogate with a following `\uXXXX` low surrogate.
    fn unicode_escape(&mut self) -> Result<char, String> {
        let hi = self.hex4()?;
        if (0xD800..=0xDBFF).contains(&hi) {
            if self.bytes.get(self.pos) == Some(&b'\\')
                && self.bytes.get(self.pos + 1) == Some(&b'u')
            {
                self.pos += 2;
                let lo = self.hex4()?;
                let c = 0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00);
                return char::from_u32(c).ok_or_else(|| "bad surrogate pair".to_string());
            }
            return Err("lone high surrogate".to_string());
        }
        char::from_u32(hi).ok_or_else(|| "bad code point".to_string())
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let slice = self
            .bytes
            .get(self.pos..self.pos + 4)
            .ok_or_else(|| "short \\u escape".to_string())?;
        let text = std::str::from_utf8(slice).map_err(|e| e.to_string())?;
        let n = u32::from_str_radix(text, 16).map_err(|e| e.to_string())?;
        self.pos += 4;
        Ok(n)
    }

    fn array(&mut self) -> Result<Value, String> {
        self.pos += 1; // consume '['
        let mut items = Vec::new();
        self.skip_ws();
        if self.bytes.get(self.pos) == Some(&b']') {
            self.pos += 1;
            return Ok(Value::Arr(items));
        }
        loop {
            self.skip_ws();
            items.push(self.value()?);
            self.skip_ws();
            match self.bytes.get(self.pos) {
                Some(b',') => self.pos += 1,
                Some(b']') => {
                    self.pos += 1;
                    return Ok(Value::Arr(items));
                }
                other => return Err(format!("expected , or ] but got {other:?}")),
            }
        }
    }

    fn object(&mut self) -> Result<Value, String> {
        self.pos += 1; // consume '{'
        let mut pairs = Vec::new();
        self.skip_ws();
        if self.bytes.get(self.pos) == Some(&b'}') {
            self.pos += 1;
            return Ok(Value::Obj(pairs));
        }
        loop {
            self.skip_ws();
            if self.bytes.get(self.pos) != Some(&b'"') {
                return Err(format!("expected object key at byte {}", self.pos));
            }
            let key = self.string()?;
            self.skip_ws();
            if self.bytes.get(self.pos) != Some(&b':') {
                return Err(format!("expected : after key {key:?}"));
            }
            self.pos += 1;
            self.skip_ws();
            let val = self.value()?;
            pairs.push((key, val));
            self.skip_ws();
            match self.bytes.get(self.pos) {
                Some(b',') => self.pos += 1,
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(Value::Obj(pairs));
                }
                other => return Err(format!("expected , or }} but got {other:?}")),
            }
        }
    }
}

/// Byte length of a UTF-8 code point from its leading byte.
fn utf8_len(lead: u8) -> usize {
    match lead {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    }
}
