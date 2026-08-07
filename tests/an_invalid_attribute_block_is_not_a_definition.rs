//! AN INVALID BLOCK IS NOT `attributes`, SO THE LINE IS NOT A DEFINITION
//! (PART 7, `reference_definition`, markup-carve/carve#933).
//!
//! `[space, attributes]` names the `attributes` production, and a balanced
//! `{...}` that production does not accept is not an instance of it. It is
//! leftover content, and the end-of-line anchor (carve#911, carve-rs#766)
//! disposes of it like any other leftover: the line falls back to prose.
//!
//! WHY THE ANCHOR ALONE COULD NOT SEE IT. The trailing block is peeled off by a
//! BALANCE SCAN before anything validates it, so a block that failed validation
//! had already been consumed and DISCARDED, and the line went on to parse as a
//! definition with the author's braces gone from the page - the exact outcome
//! PART 7 exists to avoid. Where "the block was rejected" and "there was no
//! block" are the same value, the failure has nowhere to be observed, so the
//! remedy is structural: the scan hands a rejected block BACK as content.
//!
//! THE DECIDING ARGUMENT is that the same characters already read this way one
//! construct away, which is asserted below rather than cited.

fn html(src: &str) -> String {
    carve::to_html(src).trim().to_string()
}

// ---------------------------------------------------------------------------
// The three spellings the clause names.
// ---------------------------------------------------------------------------

/// `{#}` has no identifier after the `#`.
#[test]
fn an_empty_id_stops_the_line_from_defining() {
    assert_eq!(
        html("[a][]\n\n[a]: /u {#}\n"),
        "<p>[a][]</p>\n<p>[a]: /u {#}</p>"
    );
}

/// `{ }` is an EMPTY block, which `attributes` does not accept either.
#[test]
fn a_space_only_block_stops_the_line_from_defining() {
    assert_eq!(
        html("[a][]\n\n[a]: /u { }\n"),
        "<p>[a][]</p>\n<p>[a]: /u { }</p>"
    );
}

/// `{=}` has no key before the `=`.
#[test]
fn a_keyless_pair_stops_the_line_from_defining() {
    assert_eq!(
        html("[a][]\n\n[a]: /u {=}\n"),
        "<p>[a][]</p>\n<p>[a]: /u {=}</p>"
    );
}

/// The block sits after a TITLE in the same production, and the line fails the
/// same way there - the anchor rejects the leftover wherever the tail begins.
#[test]
fn an_invalid_block_after_a_title_stops_the_line_too() {
    assert!(
        html("[a][]\n\n[a]: /u \"t\" {#}\n").starts_with("<p>[a][]</p>\n<p>[a]: /u "),
        "{}",
        html("[a][]\n\n[a]: /u \"t\" {#}\n")
    );
}

/// An IMAGE reference resolves the same entry, so it stops resolving for the
/// same reason.
#[test]
fn an_image_reference_stops_resolving_too() {
    assert_eq!(
        html("![alt][ex]\n\n[ex]: /i.png {#}\n"),
        "<p>![alt][ex]</p>\n<p>[ex]: /i.png {#}</p>"
    );
}

// ---------------------------------------------------------------------------
// The deciding argument, asserted.
// ---------------------------------------------------------------------------

/// `x {#}` in a paragraph keeps its braces as text, because `attributes`
/// rejects that block there too and inline content keeps what it cannot parse.
/// Two readings of `{#}` one construct apart is the thing being removed, and
/// the reading kept is the one the rest of the language already has.
#[test]
fn the_same_block_in_a_paragraph_already_read_this_way() {
    assert_eq!(html("x {#}\n"), "<p>x {#}</p>");
    assert_eq!(html("x { }\n"), "<p>x { }</p>");
    assert_eq!(html("x {=}\n"), "<p>x {=}</p>");
}

// ---------------------------------------------------------------------------
// Controls. A fix keyed on "there was a block" rather than on "the block is
// `attributes`" breaks these.
// ---------------------------------------------------------------------------

/// A VALID block still defines AND still transfers its attributes to every link
/// that resolves the label (PART 9R R1). This is the row an over-eager fix
/// breaks, and it is the reason the validation has to be the `attributes`
/// production rather than a shape test on the braces.
#[test]
fn control_a_valid_block_still_defines_and_transfers() {
    assert_eq!(
        html("[a][]\n\n[a]: /u {.x}\n"),
        "<p><a href=\"/u\" class=\"x\">a</a></p>"
    );
    assert_eq!(
        html("[a][]\n\n[a]: /u {#i}\n"),
        "<p><a href=\"/u\" id=\"i\">a</a></p>"
    );
    assert_eq!(
        html("![alt][ex]\n\n[ex]: /i.png {.wide}\n"),
        "<img src=\"/i.png\" alt=\"alt\" class=\"wide\">"
    );
}

/// The block is SCANNED rather than matched, and validation runs on what the
/// scan found - so a `}` inside a quoted value is still not the closer and the
/// block is still valid.
#[test]
fn control_a_quoted_closing_brace_still_validates() {
    assert_eq!(
        html("[a][]\n\n[a]: /u {data-x=\"}\" .c}\n"),
        "<p><a href=\"/u\" data-x=\"}\" class=\"c\">a</a></p>"
    );
}

/// With NO separator the braces were never a candidate block: they are part of
/// the DESTINATION and the line defines, unchanged. Validation must not reach
/// this shape, or a legal destination containing braces would stop defining.
#[test]
fn control_a_glued_block_stays_in_the_destination() {
    assert_eq!(
        html("[a][]\n\n[a]: /u{#}\n"),
        "<p><a href=\"/u{#}\">a</a></p>"
    );
}

/// A definition with no block at all is untouched.
#[test]
fn control_a_bare_definition_still_defines() {
    assert_eq!(html("[a][]\n\n[a]: /u\n"), "<p><a href=\"/u\">a</a></p>");
}

// ---------------------------------------------------------------------------
// A consequence, recorded rather than discovered later.
// ---------------------------------------------------------------------------

/// A floating attribute line above a definition floats PAST it, because a
/// definition is an invisible block (§15 A2a). Once the line stops defining it
/// is a VISIBLE block, so the floating line lands on it instead. That follows
/// from the ruling rather than from a second decision, and it is pinned here so
/// the next reader does not have to re-derive it.
#[test]
fn an_attribute_line_above_lands_on_the_paragraph_the_line_became() {
    assert_eq!(
        html("{.a}\n[ex]: /u {#}\n\n[E][ex] and text\n"),
        "<p class=\"a\">[ex]: /u {#}</p>\n<p>[E][ex] and text</p>"
    );
}
