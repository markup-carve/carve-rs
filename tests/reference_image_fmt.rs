//! `carve fmt` (to_carve) must preserve the reference-image invariant:
//! to_html(to_carve(x)) == to_html(x). An UNRESOLVED reference image round-trips
//! via its verbatim source, exactly like an unresolved reference link - emitting
//! `![alt]()` would change the rendered text and break the invariant.
//!
//! A resolved reference is no longer INLINED: PART 12 §10 gives the definition
//! a node, so the reference and its definition line both survive (carve-rs#631).
//! Every expectation below was re-measured against carve-js, which is
//! byte-identical. The `{#f}` rows are the one exception on purpose - carve-php
//! DROPS that attribute line and loses `id="f"` on the reparse
//! (markup-carve/carve-php#831), so those are matched against carve-js only.

#[test]
fn unresolved_reference_image_round_trips_verbatim() {
    let src = "![a][nope]";
    assert_eq!(carve::to_carve(src).trim(), "![a][nope]");
    assert_eq!(carve::to_html(&carve::to_carve(src)), carve::to_html(src));
}

#[test]
fn unresolved_reference_image_in_text_round_trips() {
    let src = "x ![a][nope] y";
    assert_eq!(carve::to_carve(src).trim(), "x ![a][nope] y");
    assert_eq!(carve::to_html(&carve::to_carve(src)), carve::to_html(src));
}

#[test]
fn resolved_reference_image_keeps_its_reference_form() {
    let src = "![alt][ref]\n\n[ref]: /u \"t\"";
    // A resolved reference image KEEPS its reference form, and the definition
    // line is written back (PART 12 §10, carve-rs#631).
    assert_eq!(
        carve::to_carve(src).trim(),
        "![alt][ref]\n\n[ref]: /u \"t\""
    );
    assert_eq!(carve::to_html(&carve::to_carve(src)), carve::to_html(src));
}

// A figure caption must serialize as an UNESCAPED `^ …` line: escaping the caret
// to `\^` only round-trips in carve-js's lenient parser; carve-rs and carve-php
// read `\^` as literal text and lose the figure. to_carve promotes image+caption
// (direct, resolved-ref, or one with a tricky title) to a figure, emitting the
// caption verbatim.
#[test]
fn resolved_reference_image_caption_is_unescaped() {
    let src = "![a][r]\n^ cap\n\n[r]: /u";
    assert_eq!(carve::to_carve(src).trim(), "![a][r]\n^ cap\n\n[r]: /u");
    assert_eq!(carve::to_html(&carve::to_carve(src)), carve::to_html(src));
}

#[test]
fn reference_image_with_attrs_caption_is_unescaped() {
    let src = "![a][r]{.c}\n^ cap\n\n[r]: /u";
    assert_eq!(carve::to_carve(src).trim(), "![a][r]{.c}\n^ cap\n\n[r]: /u");
    assert_eq!(carve::to_html(&carve::to_carve(src)), carve::to_html(src));
}

#[test]
fn direct_image_escaped_quote_title_caption_is_unescaped() {
    let src = "![a](/u \"t\\\"i\")\n^ cap";
    assert_eq!(carve::to_carve(src).trim(), "![a](/u \"t\\\"i\")\n^ cap");
    assert_eq!(carve::to_html(&carve::to_carve(src)), carve::to_html(src));
}

#[test]
fn unresolved_reference_image_caption_needs_no_escape() {
    // Not a figure - `[nope]` resolves to nothing - so the caption line
    // promotes nothing and the bare caret changes no parse. This test asserted
    // `\^ cap` from the same premise ("not a figure, SO the caret is
    // escaped"), which does not follow: PART 11 §4 escapes a character only
    // where omitting it changes the re-parse, and the assertion on the next
    // line is what proves it does not. carve-js pins the same shape bare.
    //
    // This depends on an unresolved image NOT being captionable, which is
    // currently what this engine and carve-js do and carve-php does not
    // (carve#623). If that question resolves the other way, the caret becomes
    // load-bearing here and this expectation flips back.
    let src = "![a][nope]\n^ cap";
    assert_eq!(carve::to_carve(src).trim(), "![a][nope]\n^ cap");
    assert_eq!(carve::to_html(&carve::to_carve(src)), carve::to_html(src));
}

// A leading block-attribute line (`{#id}`) is preserved when a reference-image
// figure is promoted while formatting: the figure inherits the paragraph attrs,
// matching a direct-image figure and carve-php.
#[test]
fn reference_figure_keeps_leading_attribute_line() {
    let src = "{#f}\n![a][r]\n^ cap\n\n[r]: /u";
    assert_eq!(
        carve::to_carve(src).trim(),
        "{#f}\n![a][r]\n^ cap\n\n[r]: /u"
    );
    assert_eq!(carve::to_html(&carve::to_carve(src)), carve::to_html(src));
}

#[test]
fn captionless_reference_image_keeps_leading_attribute_line() {
    // The sole-image -> block-image promotion is skipped while formatting, so the
    // paragraph keeps the `{#f}` line a bare block image could not carry. Byte
    // output matches carve-js / carve-php. (No to_html invariant assertion: an
    // attributed reference sole-image has a PRE-EXISTING HTML divergence, so the
    // round-trip changes the id independently of this change.)
    let src = "{#f}\n![a][r]\n\n[r]: /u";
    assert_eq!(carve::to_carve(src).trim(), "{#f}\n![a][r]\n\n[r]: /u");
}
