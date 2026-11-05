//! Paragraph text that starts with whitespace before a `:::` or `::` opener is
//! written with the leading whitespace dropped and the colon escaped, so it
//! reads back as a paragraph (CARVE-P11-006, CARVE-P11-050, carve-rs#3887).

use carve::ast_json::from_json;
use carve::render_carve;

fn text(value: &str) -> String {
    serde_json::json!({"type": "text", "value": value}).to_string()
}

fn paragraph(children: &[String]) -> String {
    format!(
        r#"{{"type":"paragraph","children":[{}]}}"#,
        children.join(",")
    )
}

fn write(blocks: &[String]) -> String {
    let json = format!(
        r#"{{"type":"document","srcByteLength":0,"children":[{}]}}"#,
        blocks.join(",")
    );
    render_carve(&from_json(&json).unwrap()).unwrap()
}

#[test]
fn a_colon_opener_after_leading_whitespace_is_escaped() {
    for (value, expected) in [
        (" ::: a", "\\::: a"),
        (" :::", "\\:::"),
        ("\t::: a", "\\::: a"),
        ("   ::: a", "\\::: a"),
        (" :: a", "\\:: a"),
        (" \t::: note", "\\::: note"),
    ] {
        let after = write(&[paragraph(&[text("x")]), paragraph(&[text(value)])]);
        assert_eq!(
            after,
            format!("x\n\n{expected}\n"),
            "after a paragraph: {value:?}"
        );
        let first = write(&[paragraph(&[text(value)])]);
        assert_eq!(first, format!("{expected}\n"), "first block: {value:?}");
    }
}

#[test]
fn the_leading_whitespace_of_other_openers_is_dropped_too() {
    for (value, expected) in [(" - a", "\\- a"), (" # a", "\\# a"), (" > a", "\\> a")] {
        let after = write(&[paragraph(&[text("x")]), paragraph(&[text(value)])]);
        assert_eq!(after, format!("x\n\n{expected}\n"), "{value:?}");
    }
}

#[test]
fn a_colon_opener_on_a_continuation_line_is_escaped() {
    let soft = r#"{"type":"soft_break"}"#.to_string();
    let written = write(&[paragraph(&[text("x"), soft.clone(), text(" ::: a")])]);
    assert_eq!(written, "x\n \\::: a\n");
    let written = write(&[paragraph(&[text("x"), soft, text(" :: a")])]);
    assert_eq!(written, "x\n \\:: a\n");
}

#[test]
fn an_inline_colon_after_whitespace_mid_line_stays_bare() {
    let written = write(&[paragraph(&[text("x")]), paragraph(&[text(" a ::: b")])]);
    assert_eq!(written, "x\n\na ::: b\n");
}
