//! C2-A-219: every number of a tree is a finite binary64 value, read only
//! when its text denotes that value and written in the RFC 8785 spelling.

use serde::Serialize;
use serde_json::Value;

/// Largest integer below which every integer is a binary64 value.
const EXACT: u64 = 1 << 53;

/// Writes finite `x` as RFC 8785 (ECMAScript `Number::toString`) does, with
/// `-0` as `0`.
pub(crate) fn spell(x: f64, out: &mut Vec<u8>) {
    if x == 0.0 {
        out.push(b'0');
        return;
    }
    // The shortest digits that read back as `x` and, of those, the closest;
    // `{:e}` of std is shortest but not always closest.
    let mut buffer = zmij::Buffer::new();
    let Some((neg, digits, n)) = decimal(buffer.format_finite(x).as_bytes()) else {
        return;
    };
    let k = i64::try_from(digits.len()).unwrap_or(i64::MAX);
    if neg {
        out.push(b'-');
    }
    if k <= n && n <= 21 {
        out.extend_from_slice(&digits);
        out.extend(std::iter::repeat_n(
            b'0',
            usize::try_from(n - k).unwrap_or(0),
        ));
    } else if 0 < n && n <= 21 {
        let (int, frac) = digits.split_at(usize::try_from(n).unwrap_or(0));
        out.extend_from_slice(int);
        out.push(b'.');
        out.extend_from_slice(frac);
    } else if -6 < n && n <= 0 {
        out.extend_from_slice(b"0.");
        out.extend(std::iter::repeat_n(b'0', usize::try_from(-n).unwrap_or(0)));
        out.extend_from_slice(&digits);
    } else {
        let (first, rest) = digits.split_at(1);
        out.extend_from_slice(first);
        if !rest.is_empty() {
            out.push(b'.');
            out.extend_from_slice(rest);
        }
        out.push(b'e');
        out.push(if n > 0 { b'+' } else { b'-' });
        out.extend_from_slice((n - 1).abs().to_string().as_bytes());
    }
}

/// A decimal number as sign, significant digits and the exponent `e` of
/// `0.digits * 10^e`; zero has no digits.
fn decimal(token: &[u8]) -> Option<(bool, Vec<u8>, i64)> {
    let (neg, body) = match token.split_first() {
        Some((b'-', rest)) => (true, rest),
        _ => (false, token),
    };
    let (mantissa, exp) = match body.iter().position(|b| matches!(b, b'e' | b'E')) {
        Some(at) => {
            let (mantissa, rest) = body.split_at(at);
            (mantissa, rest.get(1..))
        }
        None => (body, None),
    };
    let (int, frac): (&[u8], &[u8]) = match mantissa.iter().position(|&b| b == b'.') {
        Some(at) => {
            let (int, rest) = mantissa.split_at(at);
            match rest.get(1..) {
                Some(frac) if !frac.is_empty() => (int, frac),
                _ => return None,
            }
        }
        None => (mantissa, &[]),
    };
    let int_ok = matches!(int, [b'0'] | [b'1'..=b'9', ..]) && int.iter().all(u8::is_ascii_digit);
    if !int_ok || !frac.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let mut e: i64 = 0;
    if let Some(exp) = exp {
        let (sign, digits) = match exp.split_first() {
            Some((b'-', rest)) => (-1, rest),
            Some((b'+', rest)) => (1, rest),
            _ => (1, exp),
        };
        if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
            return None;
        }
        for d in digits {
            e = e.saturating_mul(10).saturating_add(i64::from(d - b'0'));
        }
        e = e.min(1 << 40) * sign;
    }
    let all: Vec<u8> = int.iter().chain(frac).copied().collect();
    let lead = all.iter().take_while(|&&d| d == b'0').count();
    let trail = all.iter().rev().take_while(|&&d| d == b'0').count();
    if lead == all.len() {
        return Some((false, Vec::new(), 0));
    }
    let point = i64::try_from(int.len()).unwrap_or(0) - i64::try_from(lead).unwrap_or(0);
    Some((
        neg,
        all.get(lead..all.len() - trail)?.to_vec(),
        point.saturating_add(e),
    ))
}

/// False when the JSON number `token` has no finite binary64 value, or the
/// RFC 8785 spelling of its nearest one denotes another number. A token that
/// is no JSON number is left to the JSON parser.
pub(crate) fn keeps_value(token: &[u8]) -> bool {
    let digits = token.strip_prefix(b"-").unwrap_or(token);
    if digits.len() <= 15 && digits.iter().all(u8::is_ascii_digit) {
        return true;
    }
    let Some(text) = decimal(token) else {
        return true;
    };
    if text.1.is_empty() {
        return true;
    }
    let Some(x) = std::str::from_utf8(token)
        .ok()
        .and_then(|t| t.parse::<f64>().ok())
        .filter(|x| x.is_finite() && *x != 0.0)
    else {
        return false;
    };
    let mut spelled = Vec::new();
    spell(x, &mut spelled);
    decimal(&spelled).is_some_and(|value| value == text)
}

/// False for an integer beyond 2^53 that binary64 does not keep.
pub(crate) fn integer_keeps(n: &serde_json::Number) -> bool {
    let token = match (n.as_u64(), n.as_i64()) {
        (Some(u), _) if u > EXACT => u.to_string(),
        (_, Some(i)) if i.unsigned_abs() > EXACT => i.to_string(),
        _ => return true,
    };
    keeps_value(token.as_bytes())
}

/// The pointer of the first number of `value` that [`integer_keeps`] refuses.
pub(crate) fn lost_integer(value: &Value) -> Option<String> {
    let mut pending = vec![(value, String::new())];
    while let Some((item, path)) = pending.pop() {
        match item {
            Value::Number(n) if !integer_keeps(n) => return Some(path),
            Value::Array(items) => pending.extend(
                items
                    .iter()
                    .enumerate()
                    .rev()
                    .map(|(i, v)| (v, format!("{path}/{i}"))),
            ),
            Value::Object(map) => pending.extend(
                map.iter()
                    .rev()
                    .map(|(k, v)| (v, format!("{path}/{}", escape(k)))),
            ),
            _ => {}
        }
    }
    None
}

fn escape(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

/// What a scan of JSON text found.
pub(crate) enum Scan {
    Clean,
    /// More than the bound of nested arrays and objects.
    Deep,
    /// The pointer of a number that does not keep its value.
    Lost(String),
}

struct Frame {
    array: bool,
    index: usize,
    /// The raw key text, quotes included, of the member being read.
    key: (usize, usize),
    expect_key: bool,
}

/// One pass over JSON text: the nesting bound, and the first number that
/// does not keep its value whose pointer `wanted` accepts.
pub(crate) fn scan(text: &str, depth: usize, mut wanted: impl FnMut(&str) -> bool) -> Scan {
    let bytes = text.as_bytes();
    let mut frames: Vec<Frame> = Vec::new();
    let mut i = 0;
    while let Some(&byte) = bytes.get(i) {
        match byte {
            b'"' => {
                let start = i;
                i += 1;
                while let Some(&b) = bytes.get(i) {
                    i += if b == b'\\' { 2 } else { 1 };
                    if b == b'"' {
                        break;
                    }
                }
                if let Some(top) = frames.last_mut().filter(|f| !f.array && f.expect_key) {
                    top.key = (start, i.min(bytes.len()));
                    top.expect_key = false;
                }
                continue;
            }
            b'{' | b'[' => {
                frames.push(Frame {
                    array: byte == b'[',
                    index: 0,
                    key: (0, 0),
                    expect_key: byte == b'{',
                });
                if frames.len() > depth {
                    return Scan::Deep;
                }
            }
            b'}' | b']' => {
                frames.pop();
            }
            b',' => {
                if let Some(top) = frames.last_mut() {
                    top.index += 1;
                    top.expect_key = !top.array;
                }
            }
            b'-' | b'0'..=b'9' => {
                let start = i;
                while bytes
                    .get(i)
                    .is_some_and(|b| matches!(b, b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E'))
                {
                    i += 1;
                }
                if !keeps_value(bytes.get(start..i).unwrap_or_default()) {
                    let path = pointer(text, &frames);
                    if wanted(&path) {
                        return Scan::Lost(path);
                    }
                }
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    Scan::Clean
}

fn pointer(text: &str, frames: &[Frame]) -> String {
    let mut out = String::new();
    for frame in frames {
        out.push('/');
        if frame.array {
            out.push_str(&frame.index.to_string());
        } else {
            let raw = text.get(frame.key.0..frame.key.1).unwrap_or("\"\"");
            out.push_str(&escape(
                &serde_json::from_str::<String>(raw).unwrap_or_default(),
            ));
        }
    }
    out
}

/// `serde_json`'s compact output with numbers in the RFC 8785 spelling.
struct Jcs;

impl serde_json::ser::Formatter for Jcs {
    fn write_f64<W: ?Sized + std::io::Write>(&mut self, w: &mut W, v: f64) -> std::io::Result<()> {
        let mut out = Vec::new();
        spell(v, &mut out);
        w.write_all(&out)
    }

    fn write_f32<W: ?Sized + std::io::Write>(&mut self, w: &mut W, v: f32) -> std::io::Result<()> {
        self.write_f64(w, f64::from(v))
    }
}

/// `v` as compact JSON text whose numbers are spelled as RFC 8785 does.
pub(crate) fn to_string<T: Serialize + ?Sized>(v: &T) -> String {
    let mut out = Vec::new();
    let mut ser = serde_json::Serializer::with_formatter(&mut out, Jcs);
    if v.serialize(&mut ser).is_err() {
        return String::new();
    }
    String::from_utf8(out).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{keeps_value, spell};

    fn spelled(x: f64) -> String {
        let mut out = Vec::new();
        spell(x, &mut out);
        String::from_utf8(out).unwrap_or_default()
    }

    #[test]
    fn spells_as_rfc_8785() {
        for (x, want) in [
            (1.0, "1"),
            (-0.0, "0"),
            (100.0, "100"),
            (0.1, "0.1"),
            (1e21, "1e+21"),
            (1e20, "100000000000000000000"),
            (1e-7, "1e-7"),
            (1e-6, "0.000001"),
            (5e-324, "5e-324"),
            (-1.5e-7, "-1.5e-7"),
            (1.2345678901234568e20, "123456789012345680000"),
            (123.456, "123.456"),
            (f64::MAX, "1.7976931348623157e+308"),
            (2f64.powi(60), "1152921504606847000"),
            (847_322_344_779_968.2, "847322344779968.2"),
            (0.243_213_653_564_453_12, "0.24321365356445312"),
        ] {
            assert_eq!(spelled(x), want, "{x:e}");
        }
    }

    #[test]
    fn keeps_only_what_binary64_keeps() {
        for token in [
            "0",
            "-0",
            "1.0",
            "1E2",
            "0.1",
            "1e21",
            "1e-7",
            "5e-324",
            "-1.5E-7",
            "9007199254740992",
            "1152921504606847000",
            "0e999",
            "100000000000000000000",
        ] {
            assert!(keeps_value(token.as_bytes()), "{token}");
        }
        for token in [
            "9007199254740993",
            "12345678901234567890",
            "0.10000000000000000001",
            "2e-324",
            "1e400",
            "-1e400",
            "1152921504606846976",
        ] {
            assert!(!keeps_value(token.as_bytes()), "{token}");
        }
    }
}
