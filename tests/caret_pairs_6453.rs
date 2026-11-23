//! carve#6453: a caret pair opens as usual behind an `^[` that never closes,
//! and a note that closes only through a caret pair inside it is text (C2-I-089).
use carve::api::json::call;
use serde_json::{Value, json};

fn call_ok(call_name: &str, source: &str) -> Value {
    let r: Value = serde_json::from_str(&call(
        &json!({"call": call_name, "source": source}).to_string(),
    ))
    .unwrap();
    r["ok"].clone()
}

fn paragraph_html(source: &str) -> String {
    let html = call_ok("render", source)["html"]
        .as_str()
        .unwrap()
        .to_string();
    html.split_once("\n<section")
        .map_or(html.clone(), |(p, _)| p.to_string())
}

fn formatted(source: &str) -> String {
    call_ok("format", source)["source"]
        .as_str()
        .unwrap()
        .to_string()
}

const REF: &str = "<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a>";

#[test]
fn tie_break_note_is_text() {
    assert_eq!(paragraph_html("^[[[^\\}[]]]"), "<p>^[[[^}[]]]</p>");
    assert_eq!(paragraph_html("^[^[[^]\\[]"), "<p>^[^[[^][]</p>");
}

#[test]
fn caret_pairs_open_behind_an_unclosed_note() {
    assert_eq!(
        paragraph_html("^[a [^1]\n\n[^1]: n"),
        format!("<p>^[a {REF}</p>")
    );
    assert_eq!(
        paragraph_html("x ^[a ^[b] c"),
        format!("<p>x ^[a {REF} c</p>")
    );
}

#[test]
fn caret_pairs_open_behind_a_tie_break_note() {
    assert_eq!(
        paragraph_html("^[[[^\\}[]]] [^1]\n\n[^1]: n"),
        format!("<p>^[[[^}}[]]] {REF}</p>")
    );
}

#[test]
fn a_note_that_closes_with_its_caret_pairs_ordinary_stays_a_note() {
    assert_eq!(paragraph_html("^[[[^\\}[]]] b]"), format!("<p>{REF}</p>"));
}

#[test]
fn fmt_is_a_fixed_point() {
    for source in [
        "^[[[^\\}[]]]\n",
        "^[a [^1]\n\n[^1]: n\n",
        "x ^[a ^[b] c\n",
        "^[^[[^]\\[]\n",
    ] {
        let once = formatted(source);
        assert_eq!(formatted(&once), once, "{source:?}");
    }
}
