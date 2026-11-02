//! Description-body boundaries follow §10 I2 and the definition-body production.
//! Captions below the body column continue an open paragraph; a list marker
//! that misses the body column closes it (carve#4860).
//! Fences with a closer ahead end it; fences without one fold (carve#2735).

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

/// A bullet marker below the body column closes the body and opens a list (carve#4860).
#[test]
fn a_bullet_continues_the_body() {
    assert_html(
        &format!("{BODY}- z\n"),
        "<dl><dt>t</dt><dd>d</dd></dl><ul><li>z</li></ul>",
    );
}

/// Ordered markers follow the same paragraph rule.
#[test]
fn an_ordered_marker_continues_the_body() {
    assert_html(
        &format!("{BODY}1. z\n"),
        "<dl><dt>t</dt><dd>d</dd></dl><ol><li>z</li></ol>",
    );
}

/// A task marker opens a task list the same way.
#[test]
fn a_task_marker_continues_the_body() {
    assert_html(
        &format!("{BODY}- [ ] z\n"),
        "<dl><dt>t</dt><dd>d</dd></dl><ul class=\"task-list\"><li><input type=\"checkbox\" disabled aria-label=\"z\"> z</li></ul>",
    );
}

/// A caption below the body column folds as text with its caret.
#[test]
fn a_caption_below_the_column_folds_into_the_body() {
    assert_html(
        &format!("{BODY}^ cap\n"),
        "<dl><dt>t</dt><dd>d ^ cap</dd></dl>",
    );
}

/// A FENCE DOES NOT end the body, info string or not (markup-carve/carve#2735).
/// This expectation was the other way round and pinned a reading the clause
/// contradicts: the engine read the language as an opener, so `` ``` `` folded
/// and ``` ```c ``` did not. Ruled 2026-10-05, the info string is irrelevant -
/// a flush-left fence line the body takes is the body's content either way -
/// and the expectation below is `renderDoc(parse(source))` at spec 8b68a46.
#[test]
fn a_fence_folds_into_the_body() {
    assert_html(
        &format!("{BODY}``` c\n"),
        "<dl><dt>t</dt><dd>d</dd></dl><pre><code class=\"language-c\"></code></pre>",
    );
}

/// THE BARE SPELLING, beside it, so the two cannot drift apart again.
#[test]
fn a_bare_fence_folds_into_the_body_too() {
    assert_html(
        &format!("{BODY}```\n"),
        "<dl><dt>t</dt><dd>d</dd></dl><pre><code></code></pre>",
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

/// INSIDE A QUOTE the flush-left marker carries no `>` and misses the body
/// column, so it closes the body and the quote and opens a list (carve#4860).
#[test]
fn a_marker_under_a_quoted_body_still_folds() {
    assert_html(
        "> :: t\n> :  d\n- z\n",
        "<blockquote><dl><dt>t</dt><dd>d</dd></dl></blockquote><ul><li>z</li></ul>",
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

/// A BELOW-COLUMN marker at a nonzero column never reached the body's column,
/// so it closes the body and opens a list (carve#4860).
#[test]
fn a_below_column_marker_is_not_this_band() {
    assert_html(
        ":: t\n:   d\n  - z\n",
        "<dl><dt>t</dt><dd>d</dd></dl><ul><li>z</li></ul>",
    );
}
