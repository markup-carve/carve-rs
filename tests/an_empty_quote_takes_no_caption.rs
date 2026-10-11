//! markup-carve/carve#2892, PART 9 §4d: a structural-only line resets caption
//! scope.
//!
//! A bare `>` is empty once its structural prefix is removed. It is not a blank
//! line, so the one-blank caption allowance never spends it, and the quote it
//! opens holds nothing a caption could be about. Counting it as blank let
//! `| a |` / blank / `>` / blank / `^ cap` render a captioned figure around an
//! empty quote - a host the caption was not written under.
//!
//! All three engines and the oracle agreed on the old reading, so the corpus
//! could not see it; djot.js 0.3.2 drops the `^ ` line entirely there. Keeping
//! the text as a paragraph is the safer half of that disagreement and is what
//! the importers already write.

use carve::to_html;

#[test]
fn an_empty_quote_between_a_table_and_the_marker_takes_no_caption() {
    assert_eq!(
        to_html("| a |\n\n>\n\n^ cap\n"),
        "<table>\n  <tbody>\n    <tr><td>a</td></tr>\n  </tbody>\n</table>\n\
         <blockquote>\n\n</blockquote>\n<p>^ cap</p>",
    );
}

#[test]
fn an_empty_quote_adjacent_to_the_marker_takes_no_caption_either() {
    assert_eq!(
        to_html(">\n^ cap\n"),
        "<blockquote>\n\n</blockquote>\n<p>^ cap</p>",
    );
}

#[test]
fn a_quote_with_content_still_captions_past_a_trailing_bare_marker_line() {
    assert_eq!(
        to_html("> q\n>\n\n^ cap\n"),
        "<figure>\n  <blockquote><p>q</p></blockquote>\n  <figcaption>cap</figcaption>\n</figure>",
    );
}
