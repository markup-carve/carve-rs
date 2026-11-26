//! carve-rs#4659: tie-break notes settle in a bounded number of passes, however
//! many precede a note (C2-I-089).
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

const REF: &str = "<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a>";

#[test]
fn a_note_behind_many_tie_break_notes_forms() {
    for n in [1, 8, 9, 10, 50, 300] {
        let ties = "^[[[^\\}[]]] ".repeat(n);
        let text = "^[[[^}[]]] ".repeat(n);
        assert_eq!(
            paragraph_html(&format!("{ties}x ^[a ^[b] c")),
            format!("<p>{text}x ^[a {REF} c</p>"),
            "{n} tie-break notes"
        );
        assert_eq!(
            paragraph_html(&format!("{ties}x [^1]\n\n[^1]: n")),
            format!("<p>{text}x {REF}</p>"),
            "{n} tie-break notes before a reference"
        );
    }
}

#[test]
fn tie_break_notes_nested_in_one_settle_together() {
    for n in [2, 9, 40] {
        let ties = "^[^[[^]\\[] ".repeat(n);
        let text = "^[^[[^][] ".repeat(n);
        assert_eq!(
            paragraph_html(&format!("{ties}x ^[b]")),
            format!("<p>{text}x {REF}</p>"),
            "{n} tie-break notes"
        );
    }
}

#[test]
fn a_note_with_a_demoted_caret_pair_still_forms_behind_tie_breaks() {
    let ties = "^[[[^\\}[]]] ".repeat(12);
    let html = call_ok("render", &format!("{ties}x ^[a [^1] b]"))["html"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(html.contains(REF), "{html}");
    assert!(html.contains("<p>a [^1] b<a href=\"#fnref1\""), "{html}");
}
