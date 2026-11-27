//! carve#6852: in the rounds of the reported openers only, a closer repair waits for the parse after a reported
//! construct (C2-W-088, C2-W-125), as in carve-js.

use carve::api::{Carve, WriteOptions};

fn written(src: &str) -> (String, Vec<String>) {
    let out = Carve::new()
        .format_with_diagnostics(src, &WriteOptions::default())
        .unwrap();
    let codes = out.diagnostics.iter().map(|d| d.code.to_string()).collect();
    (out.output, codes)
}

fn repeated(first: &str, rest: &str, line: &str, lines: usize) -> String {
    let tail = format!("{rest}{line}\n").repeat(lines - 1);
    format!("{first}{line}\n{tail}")
}

/// CC-2323's line as one paragraph: the authored-block fallback returns it unchanged.
#[test]
fn cc_2323_paragraph_returns_as_authored() {
    let src = repeated("", "", "[a *[a][]*[x]: /u", 682);
    let (text, codes) = written(&src);
    assert_eq!(text, src);
    assert!(!codes.iter().any(|c| c == "writer-refused"), "{codes:?}");
}

/// With the closer repairs placed in the same round, this item spent the repair budget and was refused.
#[test]
fn list_item_with_a_definition_fits_the_budget() {
    let src = format!(
        "{}\n[a]: /d\n",
        repeated("1. ", "   ", "{[a *[a][]*^[x]: /u", 299)
    );
    let (text, codes) = written(&src);
    assert!(!codes.iter().any(|c| c == "writer-refused"), "{codes:?}");
    assert_eq!(text, src);
}
