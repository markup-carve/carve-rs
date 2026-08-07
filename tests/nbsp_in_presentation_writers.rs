//! A no-break space is CONTENT wherever a presentation writer trims.
//!
//! carve-rs#614 fixed one site - `flatten_heading_text` on the Markdown target -
//! and left the shape of the defect in place everywhere else. `str::trim` uses
//! `char::is_whitespace`, which includes U+00A0, and the plain-text, Markdown and
//! ANSI writers each reached for it to drop the layout around a rendered
//! fragment. Three fragments were affected, in three writers:
//!
//!   * a footnote definition's body   (plain, markdown, ansi)
//!   * a table cell's content         (plain, markdown, ansi)
//!   * a figure caption               (ansi)
//!
//! The canonical writer had already learned this and kept a private copy of the
//! ASCII-only trim; the copy now lives in `render_text` and all four share it.
//!
//! Only the footnote case was reachable from the corpus
//! (267-a-definition-marker-s-separator-is-a-space-and-it-is-a-run-4). The other
//! two were found by asking the same question of every construct that trims, and
//! are pinned here because nothing else looks at them.

use carve::{parse, render_ansi, render_carve, render_markdown, render_plain_text};

const NBSP: char = '\u{a0}';

fn all_targets(src: &str) -> Vec<(&'static str, String)> {
    let doc = parse(src);
    vec![
        ("plain", render_plain_text(&doc).expect("plain")),
        ("markdown", render_markdown(&doc).expect("markdown")),
        ("ansi", render_ansi(&doc).expect("ansi")),
        ("carve", render_carve(&doc).expect("carve")),
    ]
}

fn assert_kept(src: &str, what: &str) {
    for (target, out) in all_targets(src) {
        assert!(
            out.contains(NBSP),
            "{what}: the no-break space was trimmed on the {target} target: {out:?}"
        );
    }
}

#[test]
fn a_footnote_definition_body_keeps_a_leading_no_break_space() {
    assert_kept(
        &format!("x[^f]\n\n[^f]: {NBSP}note\n"),
        "footnote definition body",
    );
}

#[test]
fn a_table_cell_keeps_a_no_break_space_at_either_end() {
    assert_kept(
        &format!("| a | b |\n| {NBSP}c{NBSP} | d |\n"),
        "table cell content",
    );
}

#[test]
fn a_figure_caption_keeps_a_no_break_space_at_either_end() {
    assert_kept(
        &format!("![alt](i.png)\n^ {NBSP}caption{NBSP}\n"),
        "figure caption",
    );
}

#[test]
fn ordinary_layout_whitespace_is_still_trimmed() {
    // The boundary. If this regressed, the fix would be trimming nothing at all
    // and the writers would emit the block renderer's newlines into the middle
    // of a footnote definition line.
    let out = render_plain_text(&parse("x[^f]\n\n[^f]:   note\n")).expect("plain");
    assert!(
        out.contains("[^f]: note"),
        "ASCII padding should still collapse: {out:?}"
    );
    assert!(!out.contains("[^f]:  note"), "{out:?}");
}

#[test]
fn a_no_break_space_in_the_middle_is_untouched() {
    // Never broken - only the ends are trimmed. Pinned so a later narrowing of
    // the fix to the leading position does not go unnoticed.
    assert_kept(&format!("x[^f]\n\n[^f]: a{NBSP}b\n"), "mid-text");
}
