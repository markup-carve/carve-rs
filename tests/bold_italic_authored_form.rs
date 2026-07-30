//! The authored bold-italic spelling survives a format.
//!
//! The combined form is a single production and the nested spelling parses to the
//! same strong-wrapping-emphasis shape, so a writer that serializes the nesting
//! "literally" normalizes one into the other -- which is what carve-js and
//! carve-php did, rewriting the spelling Carve documents into one documented
//! nowhere (carve#375, PART 11 section 6).
//!
//! This engine already distinguishes them, via `EmphasisKind::BoldItalic`, and was
//! the only one getting it right. These tests pin that so it cannot drift to
//! normalization while the other two engines converge on it.

const COMBINED: &str = "/*x*/\n";
const NESTED: &str = "*/x/*\n";

#[test]
fn each_spelling_is_reproduced_byte_exactly() {
    assert_eq!(carve::to_carve(COMBINED), COMBINED);
    assert_eq!(carve::to_carve(NESTED), NESTED);
}

#[test]
fn both_spellings_render_the_same_html() {
    // Which is why the distinction has to live in the tree rather than be
    // recovered from the output.
    assert_eq!(carve::to_html(COMBINED), carve::to_html(NESTED));
}

#[test]
fn the_mid_word_form_survives() {
    // A bare `/` needs a word boundary, so the nested spelling is not the same
    // document here: the two-char token skips that guard. A writer that
    // normalized would change what this says.
    assert_eq!(carve::to_carve("a/*y*/b\n"), "a/*y*/b\n");
}

#[test]
fn italic_nested_inside_bold_italic_survives() {
    let src = "/*a /b/ c*/\n";
    assert_eq!(carve::to_carve(src), src);
    assert_eq!(carve::to_html(&carve::to_carve(src)), carve::to_html(src));
}

#[test]
fn an_ordinary_strong_is_untouched() {
    assert_eq!(carve::to_carve("*x*\n"), "*x*\n");
}

#[test]
fn every_spelling_stays_idempotent_and_meaning_preserving() {
    for src in [
        COMBINED,
        NESTED,
        "/*bold italic*/\n",
        "a/*y*/b\n",
        "/*a /b/ c*/\n",
    ] {
        let once = carve::to_carve(src);
        assert_eq!(carve::to_carve(&once), once, "not idempotent: {src:?}");
        assert_eq!(
            carve::to_html(&once),
            carve::to_html(src),
            "meaning changed: {src:?}"
        );
    }
}
