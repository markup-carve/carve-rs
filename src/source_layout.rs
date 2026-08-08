use crate::{ast_json::source_layout_positions, Document};

fn escape_json(value: &str) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '\"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < ' ' => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('\"');
    out
}

/// Encode the opt-in PART 12 §13 source-layout sidecar. Default AST JSON is unchanged.
pub fn to_source_layout_json(source: &str, doc: &Document) -> String {
    let byte_at = |offset: usize| {
        source
            .chars()
            .take(offset)
            .map(char::len_utf8)
            .sum::<usize>()
    };
    let crlf = source.matches("\r\n").count();
    let without_crlf = source.replace("\r\n", "");
    let cr = without_crlf.matches('\r').count();
    let lf = without_crlf.matches('\n').count();
    let kinds = usize::from(crlf > 0) + usize::from(cr > 0) + usize::from(lf > 0);
    let endings = if kinds == 0 {
        "none"
    } else if kinds > 1 {
        "mixed"
    } else if crlf > 0 {
        "crlf"
    } else if cr > 0 {
        "cr"
    } else {
        "lf"
    };
    let nodes = source_layout_positions(doc)
        .into_iter()
        .map(|(path, start, end)| {
            format!(
                "{{\"path\":{},\"startByte\":{},\"endByte\":{}}}",
                escape_json(&path),
                byte_at(start),
                byte_at(end)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("{{\"version\":1,\"encoding\":\"utf-8\",\"source\":{},\"lineEndings\":\"{}\",\"bom\":{},\"nodes\":[{}]}}",
        escape_json(source), endings, source.starts_with('\u{feff}'), nodes)
}
