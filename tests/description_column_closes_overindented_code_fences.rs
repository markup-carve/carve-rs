//! A DESCRIPTION RECLAIMS A CLOSER AT ITS OWN COLUMN, like every other
//! container (CARVE-P0-004; markup-carve/carve-rs#2068).
//!
//! The description was the one container where a fence opened past the body's
//! content column was not closed by a run at that column: the collector asks
//! `detect_fence_open` about each line it takes, that function wants the fence
//! run at byte 0, and an over-indented opener arrives with the body's residual
//! in front of it. So no fence was ever open, the genuine closer read as a new
//! opener with no closer of its own, and the closer and the paragraph below it
//! became verbatim payload.
//!
//! Measured against the executable spec at markup-carve/carve `5315967c`. The
//! list item, nested item, footnote body and quoted-item twins already closed
//! here, and neither the body's column nor the size of the overshoot changes the
//! answer.

use carve::{to_html, to_html_with_options, Options};

fn both_paths(src: &str) -> String {
    let html = to_html(src);
    assert_eq!(
        html,
        to_html_with_options(src, &Options::default().with_positions(true)),
        "{src:?}"
    );
    html
}

const CLOSED: &str =
    "<dl>\n  <dt>t</dt>\n  <dd>\n    <p>d</p>\n    <pre><code>a\n</code></pre>\n    <p>tail</p>\n  </dd>\n</dl>";

/// Content column 3, opener two columns past it.
#[test]
fn a_run_at_the_description_column_closes_a_fence_two_columns_past_it() {
    assert_eq!(
        both_paths(":: t\n:  d\n\n     ```\n     a\n   ```\n\n   tail\n"),
        CLOSED
    );
}

/// Content column 3, opener four columns past it: the overshoot is not a factor.
#[test]
fn a_run_at_the_description_column_closes_a_fence_four_columns_past_it() {
    assert_eq!(
        both_paths(":: t\n:  d\n\n       ```\n       a\n   ```\n\n   tail\n"),
        CLOSED
    );
}
