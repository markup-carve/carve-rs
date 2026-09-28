//! Description-body boundaries follow §10 I2 and the definition-body production.
//! List markers continue an open paragraph; captions and interrupting fences end it.

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

/// A bullet marker continues the open description paragraph.
#[test]
fn a_bullet_continues_the_body() {
    assert_html(&format!("{BODY}- z\n"), "<dl><dt>t</dt><dd>d - z</dd></dl>");
}

/// Ordered markers follow the same paragraph rule.
#[test]
fn an_ordered_marker_continues_the_body() {
    assert_html(
        &format!("{BODY}1. z\n"),
        "<dl><dt>t</dt><dd>d 1. z</dd></dl>",
    );
}

/// A task marker is literal text when no list opens.
#[test]
fn a_task_marker_continues_the_body() {
    assert_html(
        &format!("{BODY}- [ ] z\n"),
        "<dl><dt>t</dt><dd>d - [ ] z</dd></dl>",
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

/// A FENCE ends the body too. The ticket recorded this kind as already right;
/// measured, it was not - the body swallowed it and the run became INLINE code.
#[test]
fn a_fence_ends_the_body() {
    assert_html(
        &format!("{BODY}``` c\n"),
        "<dl><dt>t</dt><dd>d</dd></dl><pre><code class=\"language-c\"> </code></pre>",
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
