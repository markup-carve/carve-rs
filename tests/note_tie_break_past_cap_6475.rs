//! carve#6475, C2-I-089 with C2-I-118: past the depth cap the tie-break notes
//! still settle, so the note behind any number of them forms.

use carve::api::json::call;
use serde_json::{Value, json};

fn render(source: &str) -> String {
    let r: Value = serde_json::from_str(&call(
        &json!({"call": "render", "source": source}).to_string(),
    ))
    .expect("a JSON reply");
    r["ok"]["html"].as_str().expect("html").to_string()
}

const NOTE: &str = "<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>ok<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>";
const REF: &str = "<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a>";

#[test]
fn a_note_behind_tie_break_notes_past_the_cap_forms() {
    for n in [197, 198, 1379, 1380, 1381, 3000, 5000] {
        let source = format!("{}^[ok]\n", "^[[[^\\}[]]] ".repeat(n));
        let expected = format!("<p>{}{REF}</p>\n{NOTE}", "^[[[^}[]]] ".repeat(n));
        assert_eq!(render(&source), expected, "{n} tie-break notes");
    }
}

#[test]
fn a_closing_note_after_a_long_run_keeps_its_caret_pairs_as_text() {
    let source = format!("{}^[a ^[b] c]\n", "^[[[^\\}[]]] ".repeat(2000));
    let expected = format!(
        "<p>{}{REF}</p>\n{}",
        "^[[[^}[]]] ".repeat(2000),
        NOTE.replace(">ok<", ">a ^[b] c<")
    );
    assert_eq!(render(&source), expected);
}
