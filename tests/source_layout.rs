use carve::{parse, to_json, to_source_layout_json};

#[test]
fn source_layout_is_opt_in_and_uses_utf8_bytes() {
    let source = "\u{feff}- 😀\r\n";
    let doc = parse(source);
    let ast = to_json(&doc);
    assert!(!ast.contains("sourceLayout"));
    let layout = to_source_layout_json(source, &doc);
    assert!(layout.contains("\"version\":1"));
    assert!(layout.contains("\"lineEndings\":\"crlf\""));
    assert!(layout.contains("\"bom\":true"));
    assert!(!layout.contains(&format!("\"endByte\":{}x", source.len())));
}
