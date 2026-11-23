//! carve#6475, C2-I-089: each `^[[[^\}[]]]` is text under the tie-break, and the note behind
//! them forms, however many stand before it (below the depth cap's reach, see the ticket).

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
fn a_note_behind_tie_break_notes_forms() {
    for n in [1, 7, 8, 9, 16, 64, 500, 1000] {
        let source = format!("{}^[ok]\n", "^[[[^\\}[]]] ".repeat(n));
        let expected = format!("<p>{}{REF}</p>\n{NOTE}", "^[[[^}[]]] ".repeat(n));
        assert_eq!(render(&source), expected, "{n} tie-break notes");
    }
}
