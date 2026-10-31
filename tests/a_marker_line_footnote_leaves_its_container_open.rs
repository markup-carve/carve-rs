fn html(source: &str) -> String {
    carve::to_html(source)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("> <", "><")
}

#[test]
fn rs_11_marker_line_note() {
    assert_eq!(
        html("1. [^c]: para\n - li\n"),
        "<ol><li></li></ol><ul><li>li</li></ul>"
    );
}

#[test]
fn rs_12_marker_line_note() {
    assert_eq!(html("1. [^c]: para\n    ---\n"), "<ol><li><hr></li></ol>");
}

#[test]
fn rs_16_marker_line_note() {
    assert_eq!(html("- [^c]: para\n  - li\nx\n"), "<ul><li></li></ul>");
}

#[test]
fn rs_19_marker_line_note() {
    assert_eq!(html("- [^c]: para\n   ---\n"), "<ul><li><hr></li></ul>");
}

#[test]
fn rs_35_marker_line_note() {
    assert_eq!(
        html(":: t\n: [^c]: para\n   {#x}\ntail\n"),
        "<dl><dt>t</dt><dd></dd></dl><p>tail</p>"
    );
}

#[test]
fn rs_38_marker_line_note() {
    assert_eq!(
        html(":: t\n: [^c]: para\n   ---\n"),
        "<dl><dt>t</dt><dd><hr></dd></dl>"
    );
}

#[test]
fn rs_41_marker_line_note() {
    assert_eq!(html("1. [^c]: para\n    {#x}\n"), "<ol><li></li></ol>");
}

#[test]
fn rs_42_marker_line_note() {
    assert_eq!(
        html("1. [^c]: para\n   %% c\n     tail\n"),
        "<ol><li>tail</li></ol>"
    );
}

#[test]
fn rs_50_marker_line_note() {
    assert_eq!(
        html("- [^c]: para\n - li\n"),
        "<ul><li></li><li>li</li></ul>"
    );
}

#[test]
fn rs_60_marker_line_note() {
    assert_eq!(html("- [^c]: para\n   {#x}\n"), "<ul><li></li></ul>");
}

#[test]
fn rs_71_marker_line_note() {
    assert_eq!(
        html("- [^c]: para\n   {#x}\ntail\n"),
        "<ul><li></li></ul><p>tail</p>"
    );
}

#[test]
fn rs_76_marker_line_note() {
    assert_eq!(html("1. [^c]: para\n   - li\ntail\n"), "<ol><li></li></ol>");
}
