//! A `+` continuation marker attaches ONE block, and §17 L3's "up to the next
//! blank line, sibling marker, or a further `+`" is that block's EXTENT rather
//! than a count (ruled in markup-carve/carve#1290).
//!
//! L3 says it in capitals - the marker "attaches the FOLLOWING flush-left block
//! to that container -- ONE block of ANY kind". The measured sibling case
//! settles the reading on its own: `- a` / `+` / `- x` / `- y` is three flat
//! items in every engine, so a sibling marker ENDS the attachment rather than
//! being swallowed into a run. The only text for the other reading was L4's
//! parenthetical "block(s)", inside a clause about the first-block form.
//!
//! THE LIST FORM WAS ALREADY CORRECT in this engine and is pinned here rather
//! than changed - carve-js and carve-php attached to the boundary and are the
//! ones that move. THE BLOCK QUOTE FORM WAS NOT: it spliced the whole run into
//! the quote, so the two spellings of one clause disagreed inside this engine.
//!
//! Nothing is lost by the narrow reading. The two-marker spelling already
//! produced identical output in all three engines, so one block costs one extra
//! marker line and no expressiveness at all.

fn html(src: &str) -> String {
    carve::to_html(src)
}

// ---------------------------------------------------------------------------
// The list-item form. Already correct; these pin it.
// ---------------------------------------------------------------------------

#[test]
fn a_marker_takes_the_paragraph_and_leaves_the_quote_outside() {
    assert_eq!(
        html("- a\n+\npara\n> q\n"),
        "<ul>\n  <li>a\n    para\n  </li>\n</ul>\n<blockquote><p>q</p></blockquote>"
    );
}

#[test]
fn a_second_attached_block_takes_a_second_marker() {
    assert_eq!(
        html("- a\n+\npara\n+\n> q\n"),
        "<ul>\n  <li>a\n    para\n    <blockquote><p>q</p></blockquote>\n  </li>\n</ul>"
    );
}

#[test]
fn the_one_block_may_be_many_lines() {
    // The boundary is the EXTENT of one block, so a wrapped paragraph is not cut
    // at its first line - and the quote below it is still outside.
    assert_eq!(
        html("- a\n+\np1\np2\n> q\n"),
        "<ul>\n  <li>a\n    p1\np2\n  </li>\n</ul>\n<blockquote><p>q</p></blockquote>"
    );
}

#[test]
fn an_attached_quote_is_one_block_however_many_lines_it_holds() {
    assert_eq!(
        html("- a\n+\n> x\n> y\n- next\n"),
        "<ul>\n  <li>a\n    <blockquote><p>x\ny</p></blockquote>\n  </li>\n  <li>next</li>\n</ul>"
    );
}

#[test]
fn a_sibling_marker_ends_the_attachment() {
    // The measured case that decides the reading: three flat items, not a
    // sub-list swallowed into a run.
    assert_eq!(
        html("- a\n+\n- x\n- y\n"),
        "<ul>\n  <li>a</li>\n  <li>x</li>\n  <li>y</li>\n</ul>"
    );
}

#[test]
fn the_first_block_form_counts_the_same_way() {
    // `- +` opens an item whose body is the ONE block that follows, so a second
    // block needs its own marker - L4's "block(s)" is the loose wording.
    assert_eq!(
        html("- +\npara\n> q\n"),
        "<ul>\n  <li>para</li>\n</ul>\n<blockquote><p>q</p></blockquote>"
    );
    assert_eq!(
        html("- +\npara\n+\n> q\n"),
        "<ul>\n  <li>para\n    <blockquote><p>q</p></blockquote>\n  </li>\n</ul>"
    );
}

// ---------------------------------------------------------------------------
// The block quote form. This is what moved.
// ---------------------------------------------------------------------------

#[test]
fn the_quote_marker_takes_the_paragraph_and_leaves_the_heading_outside() {
    // It used to splice the whole run into the quote, so the heading came out
    // inside it - the boundary reading, in the one container where this engine
    // still held it.
    assert_eq!(
        html("> quoted\n+\npara\n# H\n"),
        "<blockquote>\n  <p>quoted</p>\n  <p>para</p>\n</blockquote>\n<section id=\"H\">\n  <h1>H</h1>\n</section>"
    );
}

#[test]
fn a_second_marker_brings_it_in_exactly_as_in_the_list_form() {
    assert_eq!(
        html("> quoted\n+\npara\n+\n# H\n"),
        "<blockquote>\n  <p>quoted</p>\n  <p>para</p>\n  <h1 id=\"H\">H</h1>\n</blockquote>"
    );
}

#[test]
fn the_quote_form_also_takes_a_multi_line_block_whole() {
    // ONE block, not one LINE. A list attached to a quote carries every item it
    // holds, because the whole list is the single block.
    assert_eq!(
        html("> quoted\n+\n- x\n- y\n"),
        "<blockquote>\n  <p>quoted</p>\n  <ul>\n    <li>x</li>\n    <li>y</li>\n  </ul>\n</blockquote>"
    );
}

// ---------------------------------------------------------------------------
// CONTROLS. Everything the marker already did, which measuring the block must
// not disturb.
// ---------------------------------------------------------------------------

#[test]
fn control_a_marker_with_no_attribute_line_is_unchanged() {
    assert_eq!(
        html("- a\n+\n> q\n"),
        "<ul>\n  <li>a\n    <blockquote><p>q</p></blockquote>\n  </li>\n</ul>"
    );
    assert_eq!(
        html("- a\n+\npara\n"),
        "<ul>\n  <li>a\n    para\n  </li>\n</ul>"
    );
}

#[test]
fn control_an_attribute_line_still_attributes_the_block_it_precedes() {
    assert_eq!(
        html("- a\n+\n{.x}\n> q\n"),
        "<ul>\n  <li>a\n    <blockquote class=\"x\"><p>q</p></blockquote>\n  </li>\n</ul>"
    );
}

#[test]
fn control_a_blank_line_still_bounds_the_attachment() {
    assert_eq!(
        html("> quoted\n+\npara\n\ntail\n"),
        "<blockquote>\n  <p>quoted</p>\n  <p>para</p>\n</blockquote>\n<p>tail</p>"
    );
}

#[test]
fn control_a_quote_line_after_the_attachment_still_reaches_the_quote() {
    // The attached paragraph ends in FRONT of the prefixed line - a `>` line
    // opens a block, and the marker takes one - and the line is then the
    // quote's own content again rather than a second quote. This used to be a
    // boundary test in the quote's own scan, which is what made `> a` / `+` /
    // `> q` attach nothing at all (markup-carve/carve-rs#1428). The answer here
    // is unchanged; only what produces it is.
    assert_eq!(
        html("> quoted\n+\npara\n> more\n"),
        "<blockquote>\n  <p>quoted</p>\n  <p>para</p>\n  <p>more</p>\n</blockquote>"
    );
}

#[test]
fn control_a_lone_plus_outside_a_container_is_literal() {
    assert_eq!(html("+\n"), "<p>+</p>");
}

// ---------------------------------------------------------------------------
// The two the measurement itself could get wrong. Found by review.
// ---------------------------------------------------------------------------

#[test]
fn an_attribute_run_is_measured_with_the_block_it_floats_onto() {
    // Only `parse_blocks` owns a pending-attribute slot, and the measurement is
    // a `parse_block` call - so an attribute line left to it reads as a
    // paragraph and the measurement stops in FRONT of the block the attributes
    // were written for. That attached the attribute line alone, dropped the
    // attributes and left the heading outside the quote.
    assert_eq!(
        html("> q\n+\n{.x}\n# h\n"),
        "<blockquote>\n  <p>q</p>\n  <h1 class=\"x\" id=\"h\">h</h1>\n</blockquote>"
    );
    // The list twin, which reaches the same answer down its own path.
    assert_eq!(
        html("- a\n+\n{.x}\n# h\n"),
        "<ul>\n  <li>a\n    <h1 class=\"x\" id=\"h\">h</h1>\n  </li>\n</ul>"
    );
}

#[test]
fn a_nested_attachment_terminates() {
    // The probe parses the attached block and the caller parses those lines
    // again, so a `+`-attached container holding another `+` attachment paid
    // twice per level: nested to 14 that took 1.05 s where the same document
    // cost 0.00 s before the clause was implemented, and at the nesting cap it
    // was 9.52 s against 0.22 s. Two changes fix it - a self-delimiting block is
    // measured by its CLOSER rather than by parsing its body, and a marker
    // reached under a probe splices instead of probing again.
    //
    // Asserted as OUTPUT, not as a stopwatch: a wall-clock assertion measures
    // the machine. What this pins is that a document at a depth where the
    // doubling was plainly visible still parses and still holds its innermost
    // content. The numbers live in the commit message and beside the function.
    let mut body = String::from("para\n");
    for _ in 0..16 {
        body = format!("> q\n+\n:::\n{body}:::\n");
    }
    let out = html(&body);

    assert!(out.contains("<p>para</p>"), "{out}");
    assert!(out.matches("<blockquote>").count() >= 2, "{out}");
}

#[test]
fn a_self_delimiting_block_is_measured_by_its_closer() {
    // The extent of a fence or colon container is a line-level fact, so the
    // probe reads the closer rather than walking the body. These pin that the
    // two ways of measuring agree on where the block ends.
    assert_eq!(
        html("> q\n+\n::: d\ninner\n:::\ntail\n"),
        "<blockquote>\n  <p>q</p>\n  <div class=\"d\">\n    <p>inner</p>\n  </div>\n</blockquote>\n<p>tail</p>"
    );
    assert_eq!(
        html("> q\n+\n```\ncode\n```\ntail\n"),
        "<blockquote>\n  <p>q</p>\n  <pre><code>code\n</code></pre>\n</blockquote>\n<p>tail</p>"
    );
}
