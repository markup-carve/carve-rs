//! PART 1 S4's lazy branch ends "and NOTHING closes", and that binds the lines
//! AFTER the folded one as much as the folded one itself.
//!
//! This engine folded correctly and then closed the container anyway, so the
//! defect was invisible in every shape where the flush-left line was LAST -
//! which is every shape the corpus carried (category 270 and the block-quote
//! category beside it both end on the fold). A line that comes back to the
//! container's content column is what tells the two readings apart, and it is
//! what every case below adds (markup-carve/carve#980, carve-rs#813).
//!
//! The governing parameter is an OPEN PARAGRAPH somewhere in the stack, never
//! the fence kind: a code fence body cannot hold one at all, which is the whole
//! of the asymmetry §24's A FENCED BODY IS NOT A PARAGRAPH describes. The
//! CONTROLS at the bottom are the other half of that rule and must not move.

fn html(src: &str) -> String {
    carve::to_html(src).trim().to_string()
}

#[test]
fn a_div_goes_on_collecting_after_the_fold() {
    // The ticket's shape. `a`, `d` and `b` are ONE paragraph in ONE div. This
    // engine folded `d`, closed the div, left `b` beside it and opened a second
    // empty div on the closer it then never consumed.
    assert_eq!(
        html("- x\n  :::\n  a\nd\n  b\n  :::\n"),
        "<ul>\n  <li>x\n    <div>\n      <p>a\nd\nb</p>\n    </div>\n  </li>\n</ul>"
    );
}

#[test]
fn leaving_the_div_unterminated_does_not_change_it() {
    // The closer only matters to a line that arrives after it, so the fold and
    // the reach answer the same way with no closer at all.
    assert_eq!(
        html("- x\n  :::\n  a\nd\n  b\n"),
        "<ul>\n  <li>x\n    <div>\n      <p>a\nd\nb</p>\n    </div>\n  </li>\n</ul>"
    );
}

#[test]
fn an_admonition_is_the_same_container() {
    assert_eq!(
        html("- x\n  ::: note\n  a\nd\n  b\n  :::\n"),
        "<ul>\n  <li>x\n    <aside class=\"admonition note\" aria-label=\"Note\">\n      <p>a\nd\nb</p>\n    </aside>\n  </li>\n</ul>"
    );
}

#[test]
fn the_container_kind_is_not_a_parameter() {
    // A block quote inside the item folds and stays open the same way, so the
    // `> ` prefix after the fold reaches the SAME quote rather than a second
    // one. This was two blockquotes.
    assert_eq!(
        html("- x\n  > a\nd\n  > b\n"),
        "<ul>\n  <li>x\n    <blockquote><p>a\nd\nb</p></blockquote>\n  </li>\n</ul>"
    );
}

#[test]
fn a_marker_line_quote_is_collected_through_another_path_and_answers_alike() {
    assert_eq!(
        html("- > a\nd\n  > b\n"),
        "<ul>\n  <li>\n    <blockquote><p>a\nd\nb</p></blockquote>\n  </li>\n</ul>"
    );
}

#[test]
fn a_marker_line_heading_takes_no_fold_to_resume_from() {
    // A heading on the MARKER LINE used to keep the item open for flush-left
    // text (carve#326), and this pinned the resume that followed the fold.
    // markup-carve/carve#1280 ruled that a heading leaves no open paragraph and
    // the item therefore ends, so there is no fold here to resume from: `lazy`
    // and its own indented continuation are one top-level paragraph.
    assert_eq!(
        html("- # h\nlazy\n  b\n"),
        "<ul>\n  <li>\n    <h1 id=\"h\">h</h1>\n  </li>\n</ul>\n<p>lazy\nb</p>"
    );
    // The content-column spelling is bounded too after carve#1377, so there is
    // likewise no fold to resume from.
    assert_eq!(
        html("- x\n  # h\nlazy\n  b\n"),
        "<ul>\n  <li>x\n    <h1 id=\"h\">h</h1>\n  </li>\n</ul>\n<p>lazy\nb</p>"
    );
}

#[test]
fn depth_is_not_a_parameter_either() {
    // An item holding an item holding an open div. The open-paragraph test read
    // the div only when it was the collected stream's OWN last block, so one
    // level of nesting lost the whole construct to the top level.
    assert_eq!(
        html("- x\n  - y\n    :::\n    a\nd\n    b\n    :::\n"),
        "<ul>\n  <li>x\n    <ul>\n      <li>y\n        <div>\n          <p>a\nd\nb</p>\n        </div>\n      </li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn control_an_empty_container_still_ends_at_the_flush_left_line() {
    // THE SHARP CONTROL. Nothing in the stack holds an open paragraph - the
    // admonition is empty and the item's own paragraph was closed by the
    // opener - so S4 folds nothing, the containers close, and the line AFTER
    // the flush-left one is outside too. A reader that keeps the container open
    // here has replaced one over-reach with another.
    assert_eq!(
        html("- x\n  ::: note\nd\n  b\n"),
        "<ul>\n  <li>x\n    <aside class=\"admonition note\" aria-label=\"Note\">\n\n    </aside>\n  </li>\n</ul>\n<p>d\nb</p>"
    );
}

#[test]
fn control_a_closed_container_leaves_no_open_paragraph_either() {
    // One `:::` line inverts the first case: the closer closes the paragraph
    // inside the div, so there is nothing left to fold into.
    assert_eq!(
        html("- x\n  ::: note\n  a\n  :::\nd\n  b\n"),
        "<ul>\n  <li>x\n    <aside class=\"admonition note\" aria-label=\"Note\">\n      <p>a</p>\n    </aside>\n  </li>\n</ul>\n<p>d\nb</p>"
    );
}

#[test]
fn control_a_code_fence_body_is_not_a_paragraph_at_all() {
    // The clause this one is the twin of. A verbatim body can never hold an
    // open paragraph, so the container ends at the below-column line whatever
    // follows it - the answer corpus 276 pins, unchanged here.
    assert_eq!(
        html("- ```\nx\n```\n"),
        "<ul>\n  <li>\n    <pre><code>\n</code></pre>\n  </li>\n</ul>\n<p>x\n<code></code></p>"
    );
}

#[test]
fn control_an_empty_quote_in_an_item_still_ends_it() {
    // NO OPEN PARAGRAPH, NO LAZY LINE, in the spelling §24 states it with.
    assert_eq!(
        html("- >\nlazy\n  b\n"),
        "<ul>\n  <li>\n    <blockquote>\n\n    </blockquote>\n  </li>\n</ul>\n<p>lazy\nb</p>"
    );
}

#[test]
fn control_a_closed_container_inside_an_open_one_is_still_closed() {
    // The fence stack has a DEPTH, and reading it as a yes/no answers this one
    // wrong. `:::: outer` is unterminated but its last block is the `::: inner`
    // its own closer completed, so nothing in the stack holds an open paragraph
    // and both lines are top level. A first cut at looking through an
    // unterminated fence looked through this closed one too.
    assert_eq!(
        html("- x\n  :::: outer\n  ::: inner\n  a\n  :::\nd\n  b\n"),
        "<ul>\n  <li>x\n    <div class=\"outer\">\n      <div class=\"inner\">\n        <p>a</p>\n      </div>\n    </div>\n  </li>\n</ul>\n<p>d\nb</p>"
    );
}

#[test]
fn two_unterminated_containers_are_looked_through_both() {
    // The same two fences with the inner one left open: now the stack really
    // does reach a paragraph, two containers down, and the fold is right.
    assert_eq!(
        html("- x\n  :::: outer\n  ::: inner\n  a\nd\n  b\n"),
        "<ul>\n  <li>x\n    <div class=\"outer\">\n      <div class=\"inner\">\n        <p>a\nd\nb</p>\n      </div>\n    </div>\n  </li>\n</ul>"
    );
}

#[test]
fn an_open_container_whose_last_block_is_closed_still_folds_into_its_own_paragraph() {
    // And the row between them: the closed inner div is not the outer div's
    // LAST block, so the outer div's own paragraph is what the line folds into.
    assert_eq!(
        html("- x\n  :::: outer\n  ::: inner\n  a\n  :::\n  p\nd\n  b\n"),
        "<ul>\n  <li>x\n    <div class=\"outer\">\n      <div class=\"inner\">\n        <p>a</p>\n      </div>\n      <p>p\nd\nb</p>\n    </div>\n  </li>\n</ul>"
    );
}

#[test]
fn the_div_spelling_of_the_closed_container_inside_an_open_one() {
    // A named colon fence is an ADMONITION node and a bare one is a DIV, so the
    // two travel different arms of the openness test and a case that only spells
    // it one way leaves the other arm unproved. Same shape as the pair above,
    // bare.
    assert_eq!(
        html("- x\n  ::::\n  :::\n  a\n  :::\nd\n  b\n"),
        "<ul>\n  <li>x\n    <div>\n      <div>\n        <p>a</p>\n      </div>\n    </div>\n  </li>\n</ul>\n<p>d\nb</p>"
    );
}

#[test]
fn the_div_spelling_of_two_unterminated_containers() {
    assert_eq!(
        html("- x\n  ::::\n  :::\n  a\nd\n  b\n"),
        "<ul>\n  <li>x\n    <div>\n      <div>\n        <p>a\nd\nb</p>\n      </div>\n    </div>\n  </li>\n</ul>"
    );
}

#[test]
fn control_a_colon_shaped_line_inside_a_code_fence_opens_nothing() {
    // The openness count is read from the SOURCE, so it has to carry the code
    // fence's state too: `:::` and `::::` inside a verbatim body are code text
    // and open no container. Charging them made the two properly closed divs
    // below read as open and pulled the flush-left line back into the item.
    assert_eq!(
        html("- x\n  ```\n  :::\n  ::::\n  ```\n  :::: outer\n  ::: inner\n  a\n  :::\n  ::::\nd\n"),
        "<ul>\n  <li>x\n    <pre><code>:::\n::::\n</code></pre>\n    <div class=\"outer\">\n      <div class=\"inner\">\n        <p>a</p>\n      </div>\n    </div>\n  </li>\n</ul>\n<p>d</p>"
    );
}

#[test]
fn control_a_fenced_colon_shape_does_not_reopen_a_closed_admonition() {
    assert_eq!(
        html("- x\n  ```\n  :::\n  ```\n  ::: note\n  a\n  :::\nd\n  b\n"),
        "<ul>\n  <li>x\n    <pre><code>:::\n</code></pre>\n    <aside class=\"admonition note\" aria-label=\"Note\">\n      <p>a</p>\n    </aside>\n  </li>\n</ul>\n<p>d\nb</p>"
    );
}

#[test]
fn control_a_colon_shaped_line_inside_a_line_block_opens_nothing() {
    // The other opaque colon container. A line block's body is verse text, so a
    // `:::`-shaped line in it opens nothing - and it is where such a line is
    // MOST likely to be content. Counting them inflated the open-fence depth and
    // pulled `d` and `b` back into the item.
    //
    // What this pins is the ITEM BOUNDARY. Where the line block itself ends
    // inside an item is a separate open question on which this engine and the
    // executable spec already differ, and this case does not settle it: both
    // agree the two flush-left lines are a top-level paragraph, which is the
    // only thing the rule under test decides.
    assert_eq!(
        html("- x\n  :::: |\n  :::\n  ::::\n  ::: note\n  a\n  :::\nd\n  b\n"),
        "<ul>\n  <li>x\n    <div class=\"line-block\">\n      <p>:::</p>\n    </div>\n    <aside class=\"admonition note\" aria-label=\"Note\">\n      <p>a</p>\n    </aside>\n  </li>\n</ul>\n<p>d\nb</p>"
    );
}

#[test]
fn a_container_nested_in_a_quote_is_reached_through_the_quote_marker() {
    // The colon fence's own lines carry the quote's `> ` prefix, so the openness
    // scan only sees the fence past it. Without stripping the marker the count
    // was zero, the quote's div read as closed, and the whole tail left the item
    // as literal text with its markers showing. This one was WRONG on main too -
    // it is the same rule at one more container of depth, and it is fixed here
    // rather than left because the scan is what this change rewrote.
    assert_eq!(
        html("- x\n  > :::\n  > a\nd\n  > b\n  > :::\n"),
        "<ul>\n  <li>x\n    <blockquote>\n      <div>\n        <p>a\nd\nb</p>\n      </div>\n    </blockquote>\n  </li>\n</ul>"
    );
}

#[test]
fn the_same_quoted_container_left_unterminated() {
    assert_eq!(
        html("- x\n  > :::\n  > a\nd\n  > b\n"),
        "<ul>\n  <li>x\n    <blockquote>\n      <div>\n        <p>a\nd\nb</p>\n      </div>\n    </blockquote>\n  </li>\n</ul>"
    );
}
