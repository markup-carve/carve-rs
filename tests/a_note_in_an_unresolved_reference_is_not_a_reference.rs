//! PART 9R R2, `A NOTE INSIDE AN UNRESOLVED REFERENCE IS NOT A REFERENCE`
//! (markup-carve/carve#1198).
//!
//! R1 degrades an unresolved reference to its literal SOURCE, so the link text
//! built for it is discarded rather than written into the document. A
//! `[^label]` use or an `^[content]` note sitting in that text therefore
//! references nothing: it draws no number, a definition it was the only use of
//! stays unreferenced and is dropped, and no endnotes section is written on its
//! account.
//!
//! This engine counted it, because it numbered footnotes before it knew whether
//! the reference had resolved. The numbering said so out loud: a lone use left
//! an endnotes section whose backlink named an id no element carries, and a
//! live use after it was numbered `fnref1-2`, a repeat of a reference the
//! document does not contain.
//!
//! WHAT IS COUNTED IS WHAT THE OUTPUT HOLDS, so the neighbors of the rule go
//! the other way and are asserted below as CONTROLS: a note in a reference that
//! DOES resolve is an ordinary reference (PART 9 §16), and a note in a
//! bracketed run that never carried a tail is ordinary too, because PART 9 §14
//! renders that run's content. A fix keyed on brackets rather than on whether
//! the text reached the reader passes every row of the rule and breaks both
//! controls.
//!
//! THE MUTATION THESE ROWS EXIST FOR is dropping the `discarded` flag in
//! `collect_footnotes_inline_scoped`, or raising it on something other than an
//! unresolved reference. Either breaks the rule rows; the second also breaks
//! the controls.

use carve::{to_html, to_json_with_options, Options};

/// The `<sup>` anchor a live FIRST reference renders as.
const NOTEREF: &str = r##"<a id="fnref1" href="#fn1" role="doc-noteref"><sup>1</sup></a>"##;

/// The endnotes region, byte for byte, holding one note with one backlink.
fn lone_endnote(body: &str) -> String {
    [
        "<section role=\"doc-endnotes\">",
        "  <hr>",
        "  <ol>",
        "    <li id=\"fn1\">",
        &format!("      <p>{body}<a href=\"#fnref1\" role=\"doc-backlink\">↩</a></p>"),
        "    </li>",
        "  </ol>",
        "</section>",
    ]
    .join("\n")
}

/// Every `number` a footnote node in the tree carries, in document order.
///
/// `null` for a node that carries none, so a row can tell "no number" apart
/// from "not in the tree at all".
fn note_numbers(source: &str) -> Vec<Option<u64>> {
    let json: serde_json::Value =
        serde_json::from_str(&to_json_with_options(source, &Options::default()))
            .expect("the AST parses");
    let mut out = Vec::new();
    walk(&json, &mut out);
    out
}

fn walk(node: &serde_json::Value, out: &mut Vec<Option<u64>>) {
    match node {
        serde_json::Value::Array(items) => {
            for item in items {
                walk(item, out);
            }
        }
        serde_json::Value::Object(map) => {
            if matches!(
                map.get("type").and_then(|t| t.as_str()),
                Some("footnote_ref" | "inline_footnote")
            ) {
                out.push(map.get("number").and_then(|n| n.as_u64()));
            }
            for value in map.values() {
                walk(value, out);
            }
        }
        _ => {}
    }
}

#[test]
fn a_lone_discarded_use_writes_no_endnotes_section() {
    let html = to_html("a [t[^1]][nope] b\n\n[^1]: n\n");
    assert_eq!(html, "<p>a [t[^1]][nope] b</p>");
    // Asserted as an ABSENCE too: the exact-bytes assertion above would also
    // hold if the section had merely moved elsewhere in the output.
    assert!(!html.contains("doc-endnotes"), "{html}");
    // And the id the old backlink named, which no element in the document is.
    assert!(!html.contains("fnref1"), "{html}");
}

#[test]
fn an_inline_note_in_a_discarded_reference_is_not_placed() {
    let html = to_html("a [t^[n]][nope] b\n");
    assert_eq!(html, "<p>a [t^[n]][nope] b</p>");
    assert!(!html.contains("doc-endnotes"), "{html}");
}

#[test]
fn the_surviving_use_is_the_first_reference_not_a_repeat() {
    let html = to_html("a [t[^1]][nope] b [^1] c\n\n[^1]: n\n");
    assert_eq!(
        html,
        format!(
            "<p>a [t[^1]][nope] b {NOTEREF} c</p>\n{}",
            lone_endnote("n")
        )
    );
    // The defect the rule names: the one noteref a reader can see used to be
    // `fnref1-2`, and the endnote carried a second backlink to a `#fnref1`
    // nothing in the document is.
    assert!(!html.contains("fnref1-2"), "{html}");
}

#[test]
fn a_live_inline_note_after_a_discarded_one_is_numbered_from_one() {
    let html = to_html("a [t^[x]][nope] b ^[y] c\n");
    assert_eq!(
        html,
        format!(
            "<p>a [t^[x]][nope] b {NOTEREF} c</p>\n{}",
            lone_endnote("y")
        )
    );
    // The discarded note's own content must not reach the endnotes either.
    assert!(!html.contains(">x<"), "{html}");
}

#[test]
fn a_collapsed_reference_with_no_definition_discards_its_use() {
    let html = to_html("a [t[^1]][] b\n\n[^1]: n\n");
    assert_eq!(html, "<p>a [t[^1]][] b</p>");
    assert!(!html.contains("doc-endnotes"), "{html}");
}

#[test]
fn a_discarded_reference_nested_inside_a_resolved_one_still_discards() {
    let html = to_html("a [x[b[^1]][nope] y][r] z\n\n[r]: /u\n\n[^1]: n\n");
    assert_eq!(html, r#"<p>a <a href="/u">x[b[^1]][nope] y</a> z</p>"#);
    assert!(!html.contains("doc-endnotes"), "{html}");
}

#[test]
fn a_discarded_reference_in_a_note_body_leaves_its_definition_unreferenced() {
    let html = to_html("a [^1] b\n\n[^1]: n [t[^2]][nope] m\n\n[^2]: two\n");
    assert_eq!(
        html,
        format!(
            "<p>a {NOTEREF} b</p>\n{}",
            lone_endnote("n [t[^2]][nope] m")
        )
    );
    // The second definition was reached only from discarded text, so it is
    // unreferenced and dropped: no `fn2` item, and none of its body.
    assert!(!html.contains("fn2"), "{html}");
    assert!(!html.contains("two"), "{html}");
}

#[test]
fn only_the_resolved_use_counts_when_a_label_is_used_both_ways() {
    let html = to_html("a [t[^1]][r] b [u[^1]][nope] c\n\n[r]: /u\n\n[^1]: n\n");
    assert_eq!(
        html,
        format!(
            "<p>a <a href=\"/u\">t{NOTEREF}</a> b [u[^1]][nope] c</p>\n{}",
            lone_endnote("n")
        )
    );
    // One use reached the reader, so the endnote carries ONE backlink and the
    // numbered-backlink form (`↩<sup>1</sup>`) is not used at all.
    assert!(!html.contains("fnref1-2"), "{html}");
}

#[test]
fn the_rule_holds_in_every_container_the_reference_can_sit_in() {
    assert_eq!(
        to_html("# h [t[^1]][nope]\n\n[^1]: n\n"),
        "<section id=\"h-t\">\n  <h1>h [t[^1]][nope]</h1>\n</section>"
    );
    assert_eq!(
        to_html("- [t[^1]][nope]\n\n[^1]: n\n"),
        "<ul>\n  <li>[t[^1]][nope]</li>\n</ul>"
    );
    assert_eq!(
        to_html("> [t[^1]][nope]\n\n[^1]: n\n"),
        "<blockquote><p>[t[^1]][nope]</p></blockquote>"
    );
    assert_eq!(
        to_html("| a |\n| --- |\n| [t[^1]][nope] |\n\n[^1]: n\n"),
        [
            "<table>",
            "  <thead><tr><th scope=\"col\">a</th></tr></thead>",
            "  <tbody>",
            "    <tr><td>[t[^1]][nope]</td></tr>",
            "  </tbody>",
            "</table>",
        ]
        .join("\n")
    );
}

#[test]
fn a_reference_image_never_held_a_note_to_discard() {
    // An image's alt is a STRING rather than an inline tree, so this spelling
    // could not carry a note either before or after the rule. It is measured
    // rather than assumed, because "the image arm is unaffected" is exactly the
    // kind of claim that is true until the alt becomes a tree.
    let html = to_html("a ![t[^1]][nope] b\n\n[^1]: n\n");
    assert_eq!(html, "<p>a ![t[^1]][nope] b</p>");
    assert!(!html.contains("doc-endnotes"), "{html}");
}

#[test]
fn the_resolved_ast_agrees_with_the_rendering() {
    // PART 12 §5 keeps footnote numbering a resolution RESULT that reaches the
    // wire, so the tree has to land on the same answer the HTML does rather
    // than publish a number no rendering will use.
    assert_eq!(note_numbers("a [t[^1]][nope] b\n\n[^1]: n\n"), vec![None]);
    assert_eq!(
        note_numbers("a [t[^1]][nope] b [^1] c\n\n[^1]: n\n"),
        vec![None, Some(1)]
    );
}

#[test]
fn control_a_bracketed_run_with_no_tail_is_not_a_reference() {
    // PART 9 §14 renders that run's content, so the note in it reached the
    // reader and counts.
    assert_eq!(
        to_html("a [t[^1]] b\n\n[^1]: n\n"),
        format!("<p>a [t{NOTEREF}] b</p>\n{}", lone_endnote("n"))
    );
}

#[test]
fn control_a_note_in_a_reference_that_resolves_is_an_ordinary_reference() {
    // PART 9 §16: the resolved link text IS written.
    assert_eq!(
        to_html("a [t[^1]][r] b\n\n[r]: /u\n\n[^1]: n\n"),
        format!(
            "<p>a <a href=\"/u\">t{NOTEREF}</a> b</p>\n{}",
            lone_endnote("n")
        )
    );
}

#[test]
fn control_a_note_in_an_inline_link_is_an_ordinary_reference() {
    assert_eq!(
        to_html("a [t[^1]](/u) b\n\n[^1]: n\n"),
        format!(
            "<p>a <a href=\"/u\">t{NOTEREF}</a> b</p>\n{}",
            lone_endnote("n")
        )
    );
}

/// A reference tail FRAMES its link's text; it does not seal it
/// (markup-carve/carve#1196, corpus category 313). These rows are the second
/// half of the same seam: the numbering pass above has to descend into a
/// reference link's children, and the resolver has to have already done so.
mod a_reference_tail_does_not_seal_its_own_text {
    use super::*;

    #[test]
    fn a_reference_inside_a_reference_link_s_text_resolves() {
        // LINKS NEVER NEST (PART 3), so the inner anchor unwraps to its display
        // text. It had to become a link first to be unwrapped: before the fix
        // it stayed literal `[x][r2]` in the output.
        assert_eq!(
            to_html("a [t[x][r2]][r] b\n\n[r]: /u\n\n[r2]: /v\n"),
            r#"<p>a <a href="/u">tx</a> b</p>"#
        );
    }

    #[test]
    fn an_image_reference_inside_a_reference_link_s_text_resolves() {
        assert_eq!(
            to_html("a [t![z][r2]][r] b\n\n[r]: /u\n\n[r2]: /i.png\n"),
            r#"<p>a <a href="/u">t<img src="/i.png" alt="z"></a> b</p>"#
        );
    }

    #[test]
    fn control_the_inline_destination_spelling_already_agreed() {
        // The two spellings frame the same text, so this row is what made the
        // reference spelling's answer wrong rather than merely different.
        assert_eq!(
            to_html("a [t[x][r2]](/u) b\n\n[r2]: /v\n"),
            r#"<p>a <a href="/u">tx</a> b</p>"#
        );
    }
}
