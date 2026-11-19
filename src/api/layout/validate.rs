//! The closed version-2 layout record reader (C2-L-007 to C2-L-058).
// Four-byte base64 groups are checked before indexing.
#![allow(clippy::indexing_slicing)]

use super::super::Error;
use serde_json::{Map, Value};

fn fail(path: impl Into<String>, reason: impl Into<String>) -> Error {
    Error::Layout {
        path: path.into(),
        reason: reason.into(),
    }
}

fn object<'a>(value: &'a Value, path: &str) -> Result<&'a Map<String, Value>, Error> {
    value
        .as_object()
        .ok_or_else(|| fail(path, "is not an object"))
}

fn count(value: Option<&Value>, path: &str, min: u64) -> Result<u64, Error> {
    value
        .and_then(Value::as_u64)
        .filter(|n| *n >= min)
        .ok_or_else(|| fail(path, format!("is not an integer of at least {min}")))
}

fn string<'a>(value: Option<&'a Value>, path: &str) -> Result<&'a str, Error> {
    value
        .and_then(Value::as_str)
        .ok_or_else(|| fail(path, "is not a string"))
}

fn pointer(value: Option<&Value>, path: &str) -> Result<String, Error> {
    let text = string(value, path)?;
    if (!text.is_empty() && !text.starts_with('/'))
        || text
            .as_bytes()
            .iter()
            .enumerate()
            .any(|(at, b)| *b == b'~' && !matches!(text.as_bytes().get(at + 1), Some(b'0' | b'1')))
    {
        return Err(fail(path, "is not a JSON Pointer"));
    }
    Ok(text.to_string())
}

fn one_of(value: Option<&Value>, path: &str, allowed: &[&str]) -> Result<(), Error> {
    if value
        .and_then(Value::as_str)
        .is_some_and(|v| allowed.contains(&v))
    {
        Ok(())
    } else {
        Err(fail(path, format!("is not one of {}", allowed.join(", "))))
    }
}

fn base64_len(text: &str) -> Result<usize, Error> {
    let bytes = text.as_bytes();
    if bytes.len() % 4 != 0 {
        return Err(fail(
            "/source",
            "is not base64: its length is not a multiple of 4",
        ));
    }
    let mut decoded = Vec::with_capacity(bytes.len() / 4 * 3);
    for (index, group) in bytes.chunks_exact(4).enumerate() {
        let last = index + 1 == bytes.len() / 4;
        let digit = |byte: u8| -> Option<u8> {
            match byte {
                b'A'..=b'Z' => Some(byte - b'A'),
                b'a'..=b'z' => Some(byte - b'a' + 26),
                b'0'..=b'9' => Some(byte - b'0' + 52),
                b'+' => Some(62),
                b'/' => Some(63),
                _ => None,
            }
        };
        let a = digit(group[0]).ok_or_else(|| fail("/source", "is not base64"))?;
        let b = digit(group[1]).ok_or_else(|| fail("/source", "is not base64"))?;
        let pad2 = group[2] == b'=';
        let pad3 = group[3] == b'=';
        if pad2 && !pad3 || pad3 && !last {
            return Err(fail("/source", "is not base64"));
        }
        let c = if pad2 {
            0
        } else {
            digit(group[2]).ok_or_else(|| fail("/source", "is not base64"))?
        };
        let d = if pad3 {
            0
        } else {
            digit(group[3]).ok_or_else(|| fail("/source", "is not base64"))?
        };
        decoded.push(a << 2 | b >> 4);
        if !pad2 {
            decoded.push(b << 4 | c >> 2);
        }
        if !pad3 {
            decoded.push(c << 6 | d);
        }
    }
    if std::str::from_utf8(&decoded).is_ok() {
        return Err(fail(
            "/encoding",
            "is base64 for bytes that are well-formed UTF-8",
        ));
    }
    Ok(decoded.len())
}

const SLOTS: &[&str] = &["title", "children", "caption", "columns"];

fn segment_order(a: &str, b: &str) -> std::cmp::Ordering {
    if let (Ok(a), Ok(b)) = (a.parse::<usize>(), b.parse::<usize>()) {
        return a.cmp(&b);
    }
    let rank = |v: &str| SLOTS.iter().position(|x| *x == v).unwrap_or(SLOTS.len());
    rank(a).cmp(&rank(b)).then_with(|| a.cmp(b))
}

fn document_order(a: &str, b: &str) -> std::cmp::Ordering {
    let mut left = a.split('/');
    let mut right = b.split('/');
    loop {
        match (left.next(), right.next()) {
            (Some(a), Some(b)) => {
                let order = segment_order(a, b);
                if !order.is_eq() {
                    return order;
                }
            }
            (None, Some(_)) => return std::cmp::Ordering::Less,
            (Some(_), None) => return std::cmp::Ordering::Greater,
            (None, None) => return std::cmp::Ordering::Equal,
        }
    }
}

fn wraps(value: &Value, path: &str, span: u64) -> Result<(), Error> {
    let items = value
        .as_array()
        .ok_or_else(|| fail(path, "is not a list"))?;
    let mut previous = 0;
    for (index, item) in items.iter().enumerate() {
        let at = format!("{path}/{index}");
        let wrap = object(item, &at)?;
        for key in wrap.keys() {
            if !["line", "path", "offset", "row"].contains(&key.as_str()) {
                return Err(fail(format!("{at}/{key}"), "is not a field of a wrap"));
            }
        }
        let line = count(wrap.get("line"), &format!("{at}/line"), 1)?;
        if line < previous {
            return Err(fail(format!("{at}/line"), "is not sorted by line"));
        }
        previous = line;
        pointer(wrap.get("path"), &format!("{at}/path"))?;
        let offset = count(wrap.get("offset"), &format!("{at}/offset"), 0)?;
        if span > 0 && offset > span {
            return Err(fail(
                format!("{at}/offset"),
                "is past the source span of its cell",
            ));
        }
        if wrap.contains_key("row") {
            count(wrap.get("row"), &format!("{at}/row"), 0)?;
        }
    }
    Ok(())
}

fn entry(value: &Value, path: &str, length: usize) -> Result<String, Error> {
    let entry = object(value, path)?;
    let name = pointer(entry.get("path"), &format!("{path}/path"))?;
    let start = count(entry.get("startByte"), &format!("{path}/startByte"), 0)?;
    let end = count(entry.get("endByte"), &format!("{path}/endByte"), 0)?;
    if start > end {
        return Err(fail(format!("{path}/startByte"), "is behind endByte"));
    }
    if end > length as u64 {
        return Err(fail(
            format!("{path}/endByte"),
            "is past the end of the source",
        ));
    }
    for (key, value) in entry {
        let at = format!("{path}/{key}");
        match key.as_str() {
            "path" | "startByte" | "endByte" => {}
            "bare" | "fenced" | "hardBreakOpener" => {
                if !value.is_boolean() {
                    return Err(fail(at, "is not a boolean"));
                }
            }
            "continuationLines" | "headerRows" | "markerColumn" | "contentColumn"
            | "blankLinesBefore" | "blankLinesAfter" => {
                count(Some(value), &at, 0)?;
            }
            "rawRef" | "markerRaw" | "openerRaw" | "closerRaw" | "metadataSeparatorRaw" => {
                string(Some(value), &at)?;
            }
            "sourceFormat" => one_of(Some(value), &at, &["pipe", "csv", "tsv"])?,
            "continuationStyle" => {
                one_of(Some(value), &at, &["lazy", "indent", "marker_line", "none"])?;
            }
            "attachment" => one_of(Some(value), &at, &["preceding", "none"])?,
            "wraps" => wraps(value, &at, end - start)?,
            "optionSpelling" => {
                let options = object(value, &at)?;
                for (name, written) in options {
                    if !["lines", "region", "rows", "cols", "shift", "code"]
                        .contains(&name.as_str())
                    {
                        return Err(fail(format!("{at}/{name}"), "is not an include option"));
                    }
                    string(Some(written), &format!("{at}/{name}"))?;
                }
            }
            _ => return Err(fail(at, "is not a field of a node entry")),
        }
    }
    if entry.contains_key("openerRaw") != entry.contains_key("closerRaw") {
        return Err(fail(path, "openerRaw and closerRaw come as a pair"));
    }
    if entry.get("openerRaw").and_then(Value::as_str) == Some("") {
        return Err(fail(format!("{path}/openerRaw"), "is empty"));
    }
    Ok(name)
}

pub(crate) fn validate(value: &Value) -> Result<(), Error> {
    let root = object(value, "")?;
    for key in [
        "version",
        "encoding",
        "source",
        "lineEndings",
        "bom",
        "nodes",
    ] {
        if !root.contains_key(key) {
            return Err(fail(format!("/{key}"), "is missing"));
        }
    }
    for key in root.keys() {
        if ![
            "version",
            "encoding",
            "source",
            "lineEndings",
            "bom",
            "nodes",
        ]
        .contains(&key.as_str())
        {
            return Err(fail(format!("/{key}"), "is not a field of the record"));
        }
    }
    if root.get("version") != Some(&Value::from(2)) {
        return Err(fail("/version", "version is not implemented"));
    }
    one_of(root.get("encoding"), "/encoding", &["utf-8", "base64"])?;
    let source = string(root.get("source"), "/source")?;
    one_of(
        root.get("lineEndings"),
        "/lineEndings",
        &["none", "lf", "crlf", "cr", "mixed"],
    )?;
    if !root.get("bom").is_some_and(Value::is_boolean) {
        return Err(fail("/bom", "is not a boolean"));
    }
    let nodes = root
        .get("nodes")
        .and_then(Value::as_array)
        .ok_or_else(|| fail("/nodes", "is not a list"))?;
    let length = if root.get("encoding") == Some(&Value::from("base64")) {
        base64_len(source)?
    } else {
        source.len()
    };
    let mut previous = None::<String>;
    for (index, node) in nodes.iter().enumerate() {
        let path = format!("/nodes/{index}");
        let name = entry(node, &path, length)?;
        if let Some(previous) = previous.as_deref() {
            if !document_order(previous, &name).is_lt() {
                return Err(fail(
                    format!("{path}/path"),
                    if previous == name {
                        "is a duplicate path"
                    } else {
                        "is not in document order"
                    },
                ));
            }
        }
        previous = Some(name);
    }
    Ok(())
}
