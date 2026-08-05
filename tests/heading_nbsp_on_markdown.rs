//! A no-break space in a heading is CONTENT, on every target.
//!
//! `flatten_heading_text` used `str::trim`, and Rust's `char::is_whitespace`
//! includes U+00A0 - so a heading whose text began with a no-break space lost it
//! on the MARKDOWN target alone. Every other target in this engine kept it, and
//! carve-js and carve-php kept it on Markdown too (carve-rs#614).
//!
//! Mid-text was never affected, because only the ends are trimmed. This is
//! specifically the leading and trailing position, which is why a paragraph, a
//! list item and a block quote were all fine: none of them go through this
//! heading-specific flatten.

use carve::{parse, render_markdown};

const NBSP: char = '\u{a0}';

fn md(src: &str) -> String {
    render_markdown(&parse(src)).expect("render")
}

#[test]
fn a_leading_no_break_space_survives() {
    let src = format!("# {NBSP}Lead\n");
    let out = md(&src);
    assert!(
        out.contains(NBSP),
        "the no-break space was trimmed away: {out:?}"
    );
    assert!(out.starts_with(&format!("# {NBSP}Lead")), "{out:?}");
}

#[test]
fn a_trailing_no_break_space_survives() {
    let src = format!("# Trail{NBSP}\n");
    assert!(md(&src).contains(NBSP), "{:?}", md(&src));
}

#[test]
fn a_mid_text_no_break_space_still_survives() {
    // Never broken - pinned so the fix is not narrowed to the leading case later.
    let src = format!("# a{NBSP}b\n");
    assert!(md(&src).contains(NBSP));
}

#[test]
fn ordinary_layout_space_is_still_trimmed() {
    // The boundary: ASCII spaces after the marker are layout, not content, and
    // must still collapse. If this regressed, the fix would be trimming nothing.
    assert_eq!(md("#   Spaced\n").trim_end(), "# Spaced");
}

#[test]
fn a_plain_heading_is_unchanged() {
    assert_eq!(md("# Plain\n").trim_end(), "# Plain");
}
