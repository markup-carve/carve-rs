use carve::{parse, parse_with_source_layout, to_json};

#[test]
fn source_layout_is_opt_in_and_uses_utf8_bytes() {
    let source = "\u{feff}- 😀\r\n";
    let plain = parse(source);
    let ast = to_json(&plain);
    assert!(!ast.contains("sourceLayout"));
    let (_doc, layout) = parse_with_source_layout(source);
    assert!(layout.contains("\"version\":1"));
    assert!(layout.contains("\"lineEndings\":\"crlf\""));
    assert!(layout.contains("\"bom\":true"));
    assert!(layout.contains("\"path\":\"/children/0\""));
    assert!(!layout.contains(&format!("\"endByte\":{}x", source.len())));
}
