//! Tree diff and patch: a JSON Patch (RFC 6902) over the encoded tree,
//! addressed with RFC 6901 pointers as diagnostics are, and the fingerprints
//! that bind a patch to the tree it was made against (carve-js#5075).

use super::{Error, Tree};
use serde_json::{Value, json};
use std::fmt::Write as _;

/// The fingerprint of a source: `sha256:` and the hex SHA-256 of its UTF-8
/// bytes, so the text and the bytes of one source agree.
#[must_use]
pub fn source_fingerprint(source: &[u8]) -> String {
    format!("sha256:{}", sha256(source))
}

/// The fingerprint of a tree: `sha256:` and the hex SHA-256 of
/// [`encode_tree`](super::encode_tree).
#[must_use]
pub fn tree_fingerprint(tree: &Tree) -> String {
    source_fingerprint(super::encode_tree(tree).as_bytes())
}

/// The JSON Patch that turns the encoded tree `a` into the encoded tree `b`.
/// Applied to `a`, it gives `b`.
#[must_use]
pub fn diff_tree(a: &Tree, b: &Tree) -> Vec<Value> {
    let mut out = Vec::new();
    let (Ok(a), Ok(b)) = (wire(a), wire(b)) else {
        return out;
    };
    diff_value(&a, &b, "", &mut out);
    out
}

/// Applies a JSON Patch to the encoded tree and decodes the result.
///
/// # Errors
///
/// [`Error::Patch`] when `fingerprint` is not that of `tree`, an operation
/// fails, or the result is no tree of contract 2.0. `tree` is not changed.
pub fn apply_patch(tree: &Tree, patch: &[Value], fingerprint: Option<&str>) -> Result<Tree, Error> {
    if let Some(want) = fingerprint {
        let actual = tree_fingerprint(tree);
        if actual != want {
            return Err(refuse(
                "",
                format!("the tree has the fingerprint {actual}, not {want}"),
            ));
        }
    }
    let mut root = wire(tree)?;
    for (index, operation) in patch.iter().enumerate() {
        root = apply_operation(root, operation, index)?;
    }
    super::decode_tree(&root.to_string()).map_err(|e| match e {
        Error::Tree(t) => {
            let (at, why) = match &t {
                crate::reader::TreeError::Json(r) => (String::new(), r.clone()),
                crate::reader::TreeError::Schema { path, reason }
                | crate::reader::TreeError::Reader { path, reason }
                | crate::reader::TreeError::Decode { path, reason } => {
                    (path.clone(), reason.clone())
                }
            };
            refuse(&at, format!("the result is no tree: {why}"))
        }
        other => other,
    })
}

fn wire(tree: &Tree) -> Result<Value, Error> {
    super::writer::tree_value(&super::encode_tree(tree))
}

fn refuse(path: &str, reason: impl Into<String>) -> Error {
    Error::Patch {
        path: path.to_string(),
        reason: reason.into(),
    }
}

fn escape(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

/// Two nodes of one type are patched field by field; anything else is
/// replaced whole.
fn comparable(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Array(_), Value::Array(_)) => true,
        (Value::Object(x), Value::Object(y)) => x.get("type") == y.get("type"),
        _ => false,
    }
}

fn diff_value(a: &Value, b: &Value, path: &str, out: &mut Vec<Value>) {
    if a == b {
        return;
    }
    if !comparable(a, b) {
        out.push(json!({"op": "replace", "path": path, "value": b}));
        return;
    }
    match (a, b) {
        (Value::Array(x), Value::Array(y)) => diff_array(x, y, path, out),
        (Value::Object(x), Value::Object(y)) => {
            for key in x.keys().filter(|k| !y.contains_key(*k)) {
                out.push(json!({"op": "remove", "path": format!("{path}/{}", escape(key))}));
            }
            for (key, right) in y {
                let at = format!("{path}/{}", escape(key));
                match x.get(key) {
                    None => out.push(json!({"op": "add", "path": at, "value": right})),
                    Some(left) => diff_value(left, right, &at, out),
                }
            }
        }
        _ => {}
    }
}

/// Equal heads and tails stay; the middle is patched pairwise, then the rest
/// is removed or added.
fn diff_array(a: &[Value], b: &[Value], path: &str, out: &mut Vec<Value>) {
    let head = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let (a, b) = (a.get(head..).unwrap_or(&[]), b.get(head..).unwrap_or(&[]));
    let tail = a
        .iter()
        .rev()
        .zip(b.iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    let (left, right) = (a.len() - tail, b.len() - tail);
    let pairs = left.min(right);
    for (at, (x, y)) in a.iter().zip(b).take(pairs).enumerate() {
        diff_value(x, y, &format!("{path}/{}", head + at), out);
    }
    for _ in pairs..left {
        out.push(json!({"op": "remove", "path": format!("{path}/{}", head + pairs)}));
    }
    for (at, value) in b.iter().enumerate().take(right).skip(pairs) {
        out.push(json!({"op": "add", "path": format!("{path}/{}", head + at), "value": value}));
    }
}

fn pointer(value: Option<&Value>, at: &str) -> Result<Vec<String>, Error> {
    let Some(text) = value.and_then(Value::as_str) else {
        return Err(refuse(at, "not a JSON pointer"));
    };
    if text.is_empty() {
        return Ok(Vec::new());
    }
    let Some(rest) = text.strip_prefix('/') else {
        return Err(refuse(at, "not a JSON pointer"));
    };
    // RFC 6901: `~` only escapes `0` and `1`.
    if rest
        .match_indices('~')
        .any(|(i, _)| !matches!(rest.as_bytes().get(i + 1), Some(b'0' | b'1')))
    {
        return Err(refuse(at, "not a JSON pointer"));
    }
    Ok(rest
        .split('/')
        .map(|part| part.replace("~1", "/").replace("~0", "~"))
        .collect())
}

/// The index of `key` in an array of `len` items; `-` and `len` only when
/// `adding`.
fn index(key: &str, len: usize, at: &str, adding: bool) -> Result<usize, Error> {
    if adding && key == "-" {
        return Ok(len);
    }
    let digits = !key.is_empty()
        && key.bytes().all(|b| b.is_ascii_digit())
        && (key == "0" || !key.starts_with('0'));
    let Some(n) = digits.then(|| key.parse::<usize>().ok()).flatten() else {
        return Err(refuse(at, format!("{key} is no index")));
    };
    if n > len || (!adding && n == len) {
        return Err(refuse(at, format!("index {key} is out of range")));
    }
    Ok(n)
}

fn walk<'v>(root: &'v Value, parts: &[String], at: &str) -> Result<&'v Value, Error> {
    let mut value = root;
    for part in parts {
        value = match value {
            Value::Array(items) => {
                let i = index(part, items.len(), at, false)?;
                items.get(i).ok_or_else(|| refuse(at, "out of range"))?
            }
            Value::Object(map) => map
                .get(part)
                .ok_or_else(|| refuse(at, format!("no member {part}")))?,
            _ => return Err(refuse(at, "the target has no members")),
        };
    }
    Ok(value)
}

fn walk_mut<'v>(root: &'v mut Value, parts: &[String], at: &str) -> Result<&'v mut Value, Error> {
    let mut value = root;
    for part in parts {
        value = match value {
            Value::Array(items) => {
                let i = index(part, items.len(), at, false)?;
                items.get_mut(i).ok_or_else(|| refuse(at, "out of range"))?
            }
            Value::Object(map) => map
                .get_mut(part)
                .ok_or_else(|| refuse(at, format!("no member {part}")))?,
            _ => return Err(refuse(at, "the target has no members")),
        };
    }
    Ok(value)
}

fn insert(
    mut root: Value,
    parts: &[String],
    value: Value,
    at: &str,
    replace: bool,
) -> Result<Value, Error> {
    let Some((last, parent)) = parts.split_last() else {
        return Ok(value);
    };
    match walk_mut(&mut root, parent, at)? {
        Value::Array(items) => {
            let i = index(last, items.len(), at, !replace)?;
            if replace {
                if let Some(slot) = items.get_mut(i) {
                    *slot = value;
                }
            } else {
                items.insert(i, value);
            }
        }
        Value::Object(map) => {
            if replace && !map.contains_key(last) {
                return Err(refuse(at, format!("no member {last}")));
            }
            map.insert(last.clone(), value);
        }
        _ => return Err(refuse(at, "the target has no members")),
    }
    Ok(root)
}

fn remove(mut root: Value, parts: &[String], at: &str) -> Result<Value, Error> {
    let Some((last, parent)) = parts.split_last() else {
        return Err(refuse(at, "the root cannot be removed"));
    };
    match walk_mut(&mut root, parent, at)? {
        Value::Array(items) => {
            let i = index(last, items.len(), at, false)?;
            items.remove(i);
        }
        Value::Object(map) => {
            if map.remove(last).is_none() {
                return Err(refuse(at, format!("no member {last}")));
            }
        }
        _ => return Err(refuse(at, "the target has no members")),
    }
    Ok(root)
}

/// RFC 6902, section 4.6: equal values, members compared without regard to
/// their order. `serde_json` maps compare that way already.
fn same(a: &Value, b: &Value) -> bool {
    a == b
}

fn apply_operation(root: Value, operation: &Value, index_: usize) -> Result<Value, Error> {
    let at = format!("/{index_}");
    let Some(entry) = operation.as_object() else {
        return Err(refuse(&at, "not an operation"));
    };
    let path_at = format!("{at}/path");
    let path = pointer(entry.get("path"), &path_at)?;
    let value = || {
        entry
            .get("value")
            .cloned()
            .ok_or_else(|| refuse(&format!("{at}/value"), "missing"))
    };
    match entry.get("op").and_then(Value::as_str) {
        Some("add") => insert(root, &path, value()?, &path_at, false),
        Some("replace") => {
            let value = value()?;
            walk(&root, &path, &path_at)?;
            insert(root, &path, value, &path_at, true)
        }
        Some("remove") => remove(root, &path, &path_at),
        Some("test") => test(root, &path, &value()?, &at),
        Some(kind @ ("move" | "copy")) => transfer(root, entry, &path, &at, kind == "move"),
        _ => Err(refuse(
            &format!("{at}/op"),
            format!(
                "unknown operation {}",
                entry
                    .get("op")
                    .map_or_else(|| "undefined".into(), Value::to_string)
            ),
        )),
    }
}

fn test(root: Value, path: &[String], value: &Value, at: &str) -> Result<Value, Error> {
    if same(walk(&root, path, &format!("{at}/path"))?, value) {
        Ok(root)
    } else {
        Err(refuse(&format!("{at}/value"), "the test failed"))
    }
}

/// `move` and `copy`: the value at `from` is inserted at `path`; a move
/// removes it first and cannot move a value into itself.
fn transfer(
    root: Value,
    entry: &serde_json::Map<String, Value>,
    path: &[String],
    at: &str,
    moving: bool,
) -> Result<Value, Error> {
    let (from_at, path_at) = (format!("{at}/from"), format!("{at}/path"));
    let from = pointer(entry.get("from"), &from_at)?;
    let moved = walk(&root, &from, &from_at)?.clone();
    let mut root = root;
    if moving {
        if path.len() > from.len() && path.starts_with(&from) {
            return Err(refuse(&path_at, "a value cannot move into itself"));
        }
        root = remove(root, &from, &from_at)?;
    }
    insert(root, path, moved, &path_at, false)
}

/// Operations as the JSON boundary reads them: a list.
pub(crate) fn operations(value: Option<&Value>) -> Result<&[Value], Error> {
    value
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| refuse("", "the patch is not a list of operations"))
}

const K: [u32; 64] = [
    0x428a_2f98,
    0x7137_4491,
    0xb5c0_fbcf,
    0xe9b5_dba5,
    0x3956_c25b,
    0x59f1_11f1,
    0x923f_82a4,
    0xab1c_5ed5,
    0xd807_aa98,
    0x1283_5b01,
    0x2431_85be,
    0x550c_7dc3,
    0x72be_5d74,
    0x80de_b1fe,
    0x9bdc_06a7,
    0xc19b_f174,
    0xe49b_69c1,
    0xefbe_4786,
    0x0fc1_9dc6,
    0x240c_a1cc,
    0x2de9_2c6f,
    0x4a74_84aa,
    0x5cb0_a9dc,
    0x76f9_88da,
    0x983e_5152,
    0xa831_c66d,
    0xb003_27c8,
    0xbf59_7fc7,
    0xc6e0_0bf3,
    0xd5a7_9147,
    0x06ca_6351,
    0x1429_2967,
    0x27b7_0a85,
    0x2e1b_2138,
    0x4d2c_6dfc,
    0x5338_0d13,
    0x650a_7354,
    0x766a_0abb,
    0x81c2_c92e,
    0x9272_2c85,
    0xa2bf_e8a1,
    0xa81a_664b,
    0xc24b_8b70,
    0xc76c_51a3,
    0xd192_e819,
    0xd699_0624,
    0xf40e_3585,
    0x106a_a070,
    0x19a4_c116,
    0x1e37_6c08,
    0x2748_774c,
    0x34b0_bcb5,
    0x391c_0cb3,
    0x4ed8_aa4a,
    0x5b9c_ca4f,
    0x682e_6ff3,
    0x748f_82ee,
    0x78a5_636f,
    0x84c8_7814,
    0x8cc7_0208,
    0x90be_fffa,
    0xa450_6ceb,
    0xbef9_a3f7,
    0xc671_78f2,
];

/// The SHA-256 of `bytes` in lower-case hex (FIPS 180-4). Every index is
/// below the fixed length of its array.
#[allow(clippy::indexing_slicing)]
fn sha256(bytes: &[u8]) -> String {
    let mut h: [u32; 8] = [
        0x6a09_e667,
        0xbb67_ae85,
        0x3c6e_f372,
        0xa54f_f53a,
        0x510e_527f,
        0x9b05_688c,
        0x1f83_d9ab,
        0x5be0_cd19,
    ];
    let mut data = bytes.to_vec();
    let bits = (bytes.len() as u64).wrapping_mul(8);
    data.push(0x80);
    while data.len() % 64 != 56 {
        data.push(0);
    }
    data.extend_from_slice(&bits.to_be_bytes());
    let mut w = [0u32; 64];
    for block in data.chunks_exact(64) {
        for (t, word) in block.chunks_exact(4).enumerate() {
            w[t] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for t in 16..64 {
            let s0 = w[t - 15].rotate_right(7) ^ w[t - 15].rotate_right(18) ^ (w[t - 15] >> 3);
            let s1 = w[t - 2].rotate_right(17) ^ w[t - 2].rotate_right(19) ^ (w[t - 2] >> 10);
            w[t] = w[t - 16]
                .wrapping_add(s0)
                .wrapping_add(w[t - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut k] = h;
        for t in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let t1 = k
                .wrapping_add(s1)
                .wrapping_add((e & f) ^ (!e & g))
                .wrapping_add(K[t])
                .wrapping_add(w[t]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let t2 = s0.wrapping_add((a & b) ^ (a & c) ^ (b & c));
            k = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (slot, v) in h.iter_mut().zip([a, b, c, d, e, f, g, k]) {
            *slot = slot.wrapping_add(v);
        }
    }
    h.iter().fold(String::with_capacity(64), |mut out, x| {
        let _ = write!(out, "{x:08x}");
        out
    })
}

#[cfg(test)]
mod tests {
    use super::sha256;

    #[test]
    fn sha256_vectors() {
        assert_eq!(
            sha256(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256(&[b'a'; 64]),
            "ffe054fe7ae0cb6dc65c3af9b61d5209f439851db43d0ba5997337df154668eb"
        );
    }
}
