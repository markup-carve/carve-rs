//! A FLUSH-LEFT OPENER BELOW A DESCRIPTION BODY ENDS IT
//! (markup-carve/carve-rs#1534).
//!
//! `collect_definition_body`'s flush-left band asked
//! `interrupts_paragraph_in_band`, which reaches `interrupts_paragraph` - and
//! §10 says outright that a list marker does NOT interrupt a paragraph. That is
//! the right answer for the question `interrupts_paragraph` is usually asked;
//! the question here is whether the line CONTINUES this body, and a marker does
//! not. The oracle keeps the two apart: `foldablePlain` excludes BULLET, an
//! ordered marker, FENCE and CAPTION alongside the visible openers, and never
//! asks the fold question about a line that fails it.
//!
//! THE FENCE ROW MOVED (markup-carve/carve#2149). A fence-shaped line is no
//! longer refused outright: the fold asks §10 I4's own question first, so one
//! with NO closer ahead is paragraph text and folds. The other kinds are
//! unchanged, and the rows below pin both halves of the fence.
//!
//! AT DOCUMENT LEVEL ONLY. Inside a container the flush-left line is that
//! container's own lazy continuation - a quote reached by its marker never
//! reaches this line - and both readers already fold it there. The rows below
//! pin the item and quote hosts as controls.
//!
//! ORACLE: the executable spec (`scripts/spec/layout.mjs` + `html.mjs`) at carve
//! `2f654da9`, spec main.

use carve::{to_html, to_html_with_options, Options};

/// The #908 guard: the facade and the position-tracking path must agree.
fn both_paths(src: &str) -> String {
    let facade = to_html(src);
    let authoritative = to_html_with_options(src, &Options::default().with_positions(true));
    assert_eq!(
        facade, authoritative,
        "the library path and the position-tracking path disagree on {src:?}"
    );
    facade
}

fn assert_html(src: &str, expected: &str) {
    let normalize = |html: &str| {
        html.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .replace(" <", "<")
    };
    assert_eq!(
        normalize(&both_paths(src)),
        normalize(expected),
        "on {src:?}"
    );
}

const BODY: &str = ":: t\n:  d\n";

/// The reported document: a bullet marker ends the body and opens a list.
#[test]
fn a_bullet_ends_the_body() {
    assert_html(
        &format!("{BODY}- z\n"),
        "<dl><dt>t</dt><dd>d</dd></dl><ul><li>z</li></ul>",
    );
}

/// An ORDERED marker goes the same way.
#[test]
fn an_ordered_marker_ends_the_body() {
    assert_html(
        &format!("{BODY}1. z\n"),
        "<dl><dt>t</dt><dd>d</dd></dl><ol><li>z</li></ol>",
    );
}

/// A TASK marker goes the same way.
#[test]
fn a_task_marker_ends_the_body() {
    assert_html(
        &format!("{BODY}- [ ] z\n"),
        "<dl><dt>t</dt><dd>d</dd></dl><ul><li>\
         <input type=\"checkbox\" disabled aria-label=\"z\"> z</li></ul>",
    );
}

/// A CAPTION ends the body and leaves a paragraph - it captions nothing here.
#[test]
fn a_caption_ends_the_body() {
    assert_html(
        &format!("{BODY}^ cap\n"),
        "<dl><dt>t</dt><dd>d</dd></dl><p>^ cap</p>",
    );
}

/// A FENCE ends the body ONLY WHEN IT HAS A CLOSER. §10 I4 reaches this fold
/// too (markup-carve/carve#2149): a fence-shaped line with no closer ahead is
/// paragraph text, so it folds like the prose below and the run becomes INLINE
/// code. An info string does not change that - the closer is what decides.
/// Corpus 479-6 pins the bare spelling, 479-7 the one with a closer.
#[test]
fn a_fence_ends_the_body() {
    assert_html(
        &format!("{BODY}``` c\nx\n```\n"),
        "<dl><dt>t</dt><dd>d</dd></dl><pre><code class=\"language-c\">x </code></pre>",
    );
}

/// The same run WITHOUT a closer folds instead, which is the half the ruling
/// moved. This is the `dd` twin of corpus 479-6.
#[test]
fn a_fence_with_no_closer_folds_into_the_body() {
    assert_html(
        &format!("{BODY}``` c\n"),
        "<dl><dt>t</dt><dd>d<code> c</code></dd></dl>",
    );
}

/// PLAIN PROSE still folds - the control that fails an overshoot ending the
/// body for every flush-left line.
#[test]
fn plain_prose_still_folds() {
    assert_html(
        &format!("{BODY}tail\n"),
        "<dl><dt>t</dt><dd>d tail</dd></dl>",
    );
}

/// A SECOND DESCRIPTION still attaches rather than ending the list.
#[test]
fn a_second_description_still_attaches() {
    assert_html(
        &format!("{BODY}:  e\n"),
        "<dl><dt>t</dt><dd>d</dd><dd>e</dd></dl>",
    );
}

/// INSIDE A QUOTE the flush-left marker is the QUOTE's lazy continuation - it
/// carries no `>` and so reaches no column inside it - and both readers fold
/// it. A band that fired below document level would move this.
#[test]
fn a_marker_under_a_quoted_body_still_folds() {
    assert_html(
        "> :: t\n> :  d\n- z\n",
        "<blockquote><dl><dt>t</dt><dd>d - z</dd></dl></blockquote>",
    );
}

/// INSIDE A LIST ITEM the item's own machinery already ends the body and opens
/// a sibling. Unchanged here, before and after.
#[test]
fn a_marker_under_an_item_hosted_body_is_a_sibling() {
    assert_html(
        "- x\n  :: t\n  :  d\n- z\n",
        "<ul><li>x<dl><dt>t</dt><dd>d</dd></dl></li><li>z</li></ul>",
    );
}

/// A BELOW-COLUMN marker at a nonzero column is not this band: it never reached
/// the body's column, so it is lazy text wherever it sits.
#[test]
fn a_below_column_marker_is_not_this_band() {
    assert_html(":: t\n:   d\n  - z\n", "<dl><dt>t</dt><dd>d - z</dd></dl>");
}
