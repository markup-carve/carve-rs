//! `caption_slot = [blank_line], caption` carries at most ONE optional blank
//! line, and PART 9 §4 says the same thing in words: a caption adjacent to its
//! host, or one blank line below it, attaches; TWO blank lines DETACH and leave
//! the `^ ` line an ordinary paragraph (markup-carve/carve-rs#830).
//!
//! The scan in `consume_caption` skipped blank lines in a LOOP, so a caption
//! attached across any number of them. It was invisible because every one of
//! the 856 corpus documents that carries a `^ ` line separates it from its host
//! by zero or one blank line, so not one of them can tell "at most one" from
//! "any number" (markup-carve/carve#991 counted them: image 11, table 3,
//! blockquote 3, display math 1, code block 1, and exactly one of the twenty
//! uses the blank-line form at all).
//!
//! One shared site served all five captionable hosts, so all five attached and
//! all five are pinned here. Each two-blank-line row is paired with its
//! one-blank-line CONTROL, which passed before the fix and must keep passing:
//! the fix narrows the allowance, it does not remove it.

fn html(src: &str) -> String {
    carve::to_html(src)
}

// --- table ---------------------------------------------------------------

const TABLE_BODY: &str = "<table>\n  <thead><tr><th scope=\"col\">a</th></tr></thead>\n  <tbody>\n    <tr><td>b</td></tr>\n  </tbody>\n</table>";

#[test]
fn one_blank_line_attaches_a_table_caption() {
    // CONTROL. Passed before the fix; no mutation of this defect touches it.
    assert_eq!(
        html("| a |\n|---|\n| b |\n\n^ cap\n"),
        "<table>\n  <caption>cap</caption>\n  <thead><tr><th scope=\"col\">a</th></tr></thead>\n  <tbody>\n    <tr><td>b</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn two_blank_lines_detach_a_table_caption() {
    assert_eq!(
        html("| a |\n|---|\n| b |\n\n\n^ cap\n"),
        format!("{TABLE_BODY}\n<p>^ cap</p>")
    );
}

// --- fenced code block ---------------------------------------------------

#[test]
fn one_blank_line_attaches_a_listing_caption() {
    // CONTROL.
    assert_eq!(
        html("```\nx\n```\n\n^ cap\n"),
        "<figure>\n  <pre><code>x\n</code></pre>\n  <figcaption>cap</figcaption>\n</figure>"
    );
}

#[test]
fn two_blank_lines_detach_a_listing_caption() {
    assert_eq!(
        html("```\nx\n```\n\n\n^ cap\n"),
        "<pre><code>x\n</code></pre>\n<p>^ cap</p>"
    );
}

// --- blockquote ----------------------------------------------------------

#[test]
fn one_blank_line_attaches_a_blockquote_caption() {
    // CONTROL. This is the one shape the corpus pins, via
    // `55-blockquote-caption-after-a-blank-line.crv`. A quote's caption is its
    // ATTRIBUTION (PART 9 §4a, carve#1159) - a different node shape, the same
    // slot and the same blank-line allowance, which is what this file is about.
    assert_eq!(
        html("> q\n\n^ cap\n"),
        "<blockquote>\n  <p>q</p>\n  <footer>cap</footer>\n</blockquote>"
    );
}

#[test]
fn two_blank_lines_detach_a_blockquote_caption() {
    assert_eq!(
        html("> q\n\n\n^ cap\n"),
        "<blockquote><p>q</p></blockquote>\n<p>^ cap</p>"
    );
}

// --- image paragraph -----------------------------------------------------

#[test]
fn one_blank_line_attaches_an_image_caption() {
    // CONTROL.
    assert_eq!(
        html("![a](i.png)\n\n^ cap\n"),
        "<figure>\n  <img src=\"i.png\" alt=\"a\">\n  <figcaption>cap</figcaption>\n</figure>"
    );
}

#[test]
fn two_blank_lines_detach_an_image_caption() {
    assert_eq!(
        html("![a](i.png)\n\n\n^ cap\n"),
        "<img src=\"i.png\" alt=\"a\">\n<p>^ cap</p>"
    );
}

// --- standalone display math ---------------------------------------------

#[test]
fn one_blank_line_attaches_an_equation_caption() {
    // CONTROL.
    assert_eq!(
        html("$$`x`\n\n^ cap\n"),
        "<figure>\n  <p><span class=\"math display\">\\[x\\]</span></p>\n  <figcaption>cap</figcaption>\n</figure>"
    );
}

#[test]
fn two_blank_lines_detach_an_equation_caption() {
    assert_eq!(
        html("$$`x`\n\n\n^ cap\n"),
        "<p><span class=\"math display\">\\[x\\]</span></p>\n<p>^ cap</p>"
    );
}

// --- more than two -------------------------------------------------------

#[test]
fn three_blank_lines_detach_too() {
    // The allowance is "at most one", not "not exactly two": a loop that
    // consumed two and stopped would pass every row above and fail here.
    assert_eq!(
        html("> q\n\n\n\n^ cap\n"),
        "<blockquote><p>q</p></blockquote>\n<p>^ cap</p>"
    );
}

#[test]
fn an_adjacent_caption_still_attaches() {
    // CONTROL. The zero-blank-line form is the common one (19 of the 20 corpus
    // documents that carry a caption) and is untouched by the slot's width.
    assert_eq!(
        html("> q\n^ cap\n"),
        "<blockquote>\n  <p>q</p>\n  <footer>cap</footer>\n</blockquote>"
    );
}
