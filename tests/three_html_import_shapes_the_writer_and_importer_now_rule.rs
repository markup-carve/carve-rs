//! The three shapes markup-carve/carve#1601 measured and markup-carve/carve#1609
//! ruled: a cell whose whole payload is a span marker, the symbol sigil, and an
//! anchor or image with no destination.
//!
//! Two of the three are PART 11 §2 escapes the WRITER was not spending. §2's
//! test is that a character is escaped IF AND ONLY IF omitting the escape would
//! change the re-parsed AST, so both were already required - nothing new was
//! ruled, an existing rule was simply not being applied to two of its cases.
//! The third is an import policy rule that is new.
//!
//! WHY BYTES ARE NOT ENOUGH HERE, and why every assertion below re-parses. The
//! first shape's failure is a DELETED CELL: written bare, `| ^ |` re-reads as a
//! rowspan marker, the cell disappears and the cell above it grows a
//! `rowspan="2"`. The second's is a `symbol` node that renders a GLYPH where the
//! document held text, and only under a configured symbol map - so an engine
//! comparing its own HTML against its own HTML sees neither.
//!
//! The shared fixtures under `tests/spec/tests/html-import` pin all three and
//! `html_import.rs` runs them. This file states the rules in this repo's own
//! terms, so a submodule bump that moved a fixture cannot quietly take the
//! behavior with it, and it carries the BOUNDS - the near neighbours that must
//! NOT move - which a fixture pinning one document has no room for.

use carve::{html_to_ast, html_to_carve, render_html, HtmlImportOptions};

fn to_carve(html: &str) -> String {
    html_to_carve(html, &HtmlImportOptions::default())
        .unwrap()
        .value
}

fn diagnostics(html: &str) -> Vec<(String, String, String)> {
    html_to_ast(html, &HtmlImportOptions::default())
        .unwrap()
        .report
        .diagnostics
        .iter()
        .map(|d| {
            (
                d.code.as_str().to_string(),
                d.message.clone(),
                d.severity.as_str().to_string(),
            )
        })
        .collect()
}

/// The written source, RE-READ, against the tree the importer built. What every
/// assertion here is really about: whether the document the writer produced
/// says what the document it was given said.
///
/// Compared through the renderer rather than as trees, because the two carry
/// different provenance - the imported tree has no source positions and the
/// re-parsed one does - and provenance is not what any of these defects is
/// about. A deleted cell, a `symbol` where text was, and four punctuation
/// characters in the prose all reach the output.
fn round_trips(html: &str) -> bool {
    let written = to_carve(html);
    let imported = html_to_ast(html, &HtmlImportOptions::default())
        .unwrap()
        .value;
    render_html(&carve::parse(&written)).unwrap() == render_html(&imported).unwrap()
}

// ---------------------------------------------------------------------------
// 1. A table cell whose whole payload is a span marker.
// ---------------------------------------------------------------------------

/// `span_cell = rowspan_marker | colspan_marker` is ONE production over TWO
/// markers, and only the `<` half was being escaped. PART 11 §6f is why the
/// cell's padding does not already cover the other: `rowspan_marker = {space},
/// '^', {space}` is written WITH the padding inside it, so the space the writer
/// puts either side of the content puts nothing out of the marker's reach.
#[test]
fn a_cell_holding_a_span_marker_is_frozen() {
    let html = "<table><tr><td>a</td><td>b</td></tr><tr><td>^</td><td>&lt;</td></tr></table>";
    assert_eq!(to_carve(html), "| a | b |\n| \\^ | \\< |\n");
    assert!(round_trips(html));
}

/// THE FAILURE THE ESCAPE PREVENTS, stated as the thing that goes wrong rather
/// than as the bytes that stop it. Written bare, the caret cell re-reads as a
/// rowspan marker: the second row loses a cell and the first row's cell spans
/// two. A byte comparison of the HTML cannot see this, which is why it survived.
#[test]
fn the_bare_form_would_delete_the_cell() {
    let deleted = carve::parse("| a | b |\n| ^ | c |\n");
    let kept = carve::parse("| a | b |\n| \\^ | c |\n");
    assert_ne!(
        deleted, kept,
        "if these agree the escape is protecting nothing"
    );
}

/// BOUND: A CARET IS ONLY A MARKER WHERE THE PARSER READS ONE. The escape is
/// decided by §2's search re-parsing the cell, not by a rule over the character,
/// so a caret with content beside it is ordinary text and keeps no escape.
/// Superscript is braced-only, so `10^6^` carries no markup anywhere.
#[test]
fn a_caret_that_is_not_the_whole_payload_stays_bare() {
    for (html, expected) in [
        ("<table><tr><td>a ^ b</td></tr></table>", "| a ^ b |\n"),
        ("<table><tr><td>10^6^</td></tr></table>", "| 10^6^ |\n"),
        ("<p>10^6^</p>", "10^6^\n"),
    ] {
        assert_eq!(to_carve(html), expected, "{html}");
        assert!(round_trips(html), "{html}");
    }
}

// ---------------------------------------------------------------------------
// 2. The symbol sigil.
// ---------------------------------------------------------------------------

/// `:` is in PART 11 §5's candidate set and `parse` yields a `symbol` node for
/// `a :rocket: b` unconditionally, so §2's test already required the escape.
/// The tag sigil beside it was already hardened; the symbol sigil was not, and
/// under a configured symbol map the text rendered as a GLYPH.
#[test]
fn the_symbol_sigil_is_frozen_beside_the_tag_sigil() {
    let html = "<p>a :rocket: b and a #t tag</p>";
    assert_eq!(to_carve(html), "a \\:rocket: b and a \\#t tag\n");
    assert!(round_trips(html));
}

/// ONLY THE OPENING COLON. Freezing the opener is what makes the whole
/// shortcode text - the closing colon then has a letter against it and opens
/// nothing - so a second escape would be bytes PART 11 §4 asks the writer not
/// to spend.
#[test]
fn only_the_opening_colon_is_frozen() {
    assert_eq!(to_carve("<p>a :rocket: b</p>").matches("\\:").count(), 1);
}

/// BOUND: THE NEAR NEIGHBOUR THAT MUST NOT MOVE. A rule over every colon would
/// freeze this one too. The parser opens no symbol at either colon here - a
/// space is not a name character - so §2's test asks for the bare form, and the
/// escaper corpus pins the same pair (`a-symbol-shortcode` against
/// `a-colon-that-closes-no-shortcode`).
#[test]
fn a_colon_that_closes_no_shortcode_stays_bare() {
    for (html, expected) in [
        ("<p>a : b : c</p>", "a : b : c\n"),
        ("<p>note: see below</p>", "note: see below\n"),
        ("<p>12:30:45</p>", "12:30:45\n"),
        ("<p>a :rocket b</p>", "a :rocket b\n"),
    ] {
        assert_eq!(to_carve(html), expected, "{html}");
        assert!(round_trips(html), "{html}");
    }
}

// ---------------------------------------------------------------------------
// 3. An anchor or image with no destination.
// ---------------------------------------------------------------------------

/// Carve has NO spelling for an empty destination: `[t]()` and `![t]()` are
/// literal text. So the importer builds no link or image node and writes what
/// the element's content and its SURVIVING attributes would produce without it
/// - the span where an attribute survives, the bare content where none does.
#[test]
fn an_element_naming_no_destination_comes_back_as_its_content() {
    let html = "<p><a href=\"\">click here</a> and <a id=\"k\">a named one</a></p>\n<img src=\"\" alt=\"logo\">";
    assert_eq!(to_carve(html), "click here and [a named one]{#k}\n\nlogo\n");
    assert!(round_trips(html));
}

/// THE RULE IS OVER THE DESTINATION, not over the reason it is missing: an
/// absent attribute and a present-but-empty one are one shape. EMPTY is read
/// the way an HTML URL attribute is read - zero length once leading and
/// trailing ASCII whitespace is stripped.
#[test]
fn absent_empty_and_blank_are_one_shape() {
    for html in [
        "<p><a>t</a></p>",
        "<p><a href=\"\">t</a></p>",
        "<p><a href=\"   \">t</a></p>",
        "<p><a href=\"\n\t \">t</a></p>",
    ] {
        assert_eq!(to_carve(html), "t\n", "{html}");
    }
}

/// AN IMAGE'S CONTENT IS ITS ALTERNATIVE TEXT - what every target with no image
/// shows for it, and what a browser shows for one it cannot load.
#[test]
fn an_images_content_is_its_alternative_text() {
    assert_eq!(to_carve("<p><img src=\"\" alt=\"logo\"></p>"), "logo\n");
    assert_eq!(to_carve("<p>a<img src=\"\">b</p>"), "ab\n");
}

/// The unwrap is LOSSY and this page requires a lossy decision to be
/// observable. It is not the bare `<div>`'s case, where nothing was lost
/// because nothing was carried: an anchor has a slot for a destination and this
/// one is standing empty.
#[test]
fn each_unwrapped_element_is_reported() {
    assert_eq!(
        diagnostics("<p><a href=\"\">t</a><img src=\"\" alt=\"l\"></p>"),
        vec![
            (
                "element-unwrapped".to_string(),
                "Unwrapped <a> with no destination".to_string(),
                "info".to_string(),
            ),
            (
                "element-unwrapped".to_string(),
                "Unwrapped <img> with no source".to_string(),
                "info".to_string(),
            ),
        ]
    );
}

/// THE SECURITY HALF. `href=""` is what PART 9 §25's URL sink denylist EMITS
/// when it blanks a dangerous scheme while keeping the visible text, so this is
/// the importer reading Carve's own hardened output. What the round trip owes
/// there is the TEXT and nothing else: the destination MUST NOT be rebuilt -
/// not from a `title`, not from the anchor's own text.
#[test]
fn a_blanked_destination_is_never_rebuilt() {
    let written = to_carve("<p><a href=\"\" title=\"javascript:alert(1)\">click</a></p>");
    assert_eq!(written, "click\n");
    assert!(!written.contains("javascript"));
    assert!(!written.contains("("));
}

/// BOUND: A DESTINATION THAT IS MERELY UNUSUAL IS NOT EMPTY, and is kept. The
/// rule reaches the empty string, not every value an importer might dislike.
#[test]
fn a_destination_that_is_only_unusual_is_kept() {
    for (html, expected) in [
        ("<p><a href=\"#\">t</a></p>", "[t](#)\n"),
        ("<p><a href=\"/\">t</a></p>", "[t](/)\n"),
        (
            "<p><a href=\"https://example.com\">t</a></p>",
            "[t](https://example.com)\n",
        ),
    ] {
        assert_eq!(to_carve(html), expected, "{html}");
    }
}
