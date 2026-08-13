//! PART 0 S4's lazy branch continues an open PARAGRAPH, and a definition body
//! that holds none does not take the fold - whatever the body is made of.
//!
//! `markup-carve/carve#956` is stated on the paragraph and its worked example
//! shows a FENCE, so the fence was fixed first (carve-rs#785 for the open half,
//! #789 for the closed one) and every other body that holds no paragraph went on
//! folding. This file is the rest of them (carve-rs#790).
//!
//! THE TICKET NAMED THREE. Seven bodies were measured to fold here while the
//! LIST twin closes, and all seven move together, because the rule is about the
//! paragraph and not about any one block kind. Each row below is asserted
//! against its list spelling, which already answers the way the clause requires.
//!
//! THE HEADING IS THE EXCEPTION and it is a CONTROL here, not a defect: a
//! heading is not a paragraph either, yet `heading_folds_lazy.rs` pins the
//! following flush-left line INSIDE the body, and pins the same answer for four
//! list spellings beside it (carve#326). The list path reaches that answer
//! without consulting `nested_ends_with_open_paragraph` at all, so the exception
//! is written where it reaches the definition body and nothing else.

fn html(src: &str) -> String {
    carve::to_html(src).trim().to_string()
}

/// The same body, spelled as a LIST item. Its answer is the one the clause
/// requires and the one this engine already gave, which is what makes each row
/// below a divergence rather than a design choice.
fn list_twin(src: &str) -> String {
    carve::to_html(src).trim().to_string()
}

#[test]
fn an_empty_block_quote_body_does_not_take_the_fold() {
    assert_eq!(
        html(":: t\n:  >\nlazy\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <blockquote>\n\n    </blockquote>\n  </dd>\n</dl>\n<p>lazy</p>"
    );
    assert!(list_twin("- >\nlazy\n").ends_with("<p>lazy</p>"));
}

#[test]
fn a_closed_admonition_body_does_not_take_the_fold() {
    assert_eq!(
        html(":: t\n:  ::: note\n   body\n   :::\nlazy\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <aside class=\"admonition note\">\n      <p>body</p>\n    </aside>\n  </dd>\n</dl>\n<p>lazy</p>"
    );
    assert!(list_twin("- ::: note\n  body\n  :::\nlazy\n").ends_with("<p>lazy</p>"));
}

#[test]
fn a_definition_attribute_is_left_empty_but_a_marker_line_attribute_floats() {
    assert_eq!(
        html(":: t\n:  {.a}\nlazy\n"),
        "<dl>\n  <dt>t</dt>\n  <dd></dd>\n</dl>\n<p>lazy</p>"
    );
    assert!(list_twin("- {.a}\nlazy\n").contains("<li><p class=\"a\">lazy</p></li>"));
}

// ---------------------------------------------------------------------------
// Four the ticket did not name. The count in a ticket is a floor: these were
// found by sweeping every body kind against its list twin rather than by
// working the list.
// ---------------------------------------------------------------------------

#[test]
fn a_table_body_does_not_take_the_fold() {
    assert_eq!(
        html(":: t\n:  | a |\n   |---|\n   | b |\nlazy\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <table>\n      <thead><tr><th scope=\"col\">a</th></tr></thead>\n      <tbody>\n        <tr><td>b</td></tr>\n      </tbody>\n    </table>\n  </dd>\n</dl>\n<p>lazy</p>"
    );
    assert!(list_twin("- | a |\n  |---|\n  | b |\nlazy\n").ends_with("<p>lazy</p>"));
}

#[test]
fn a_definition_thematic_break_does_not_fold_but_a_marker_line_one_keeps_the_item_open() {
    assert_eq!(
        html(":: t\n:  ---\nlazy\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <hr>\n  </dd>\n</dl>\n<p>lazy</p>"
    );
    let list = list_twin("- ---\nlazy\n");
    assert!(list.contains("<hr>\n    lazy\n  </li>"));
    assert!(!list.ends_with("<p>lazy</p>"));
}

#[test]
fn a_closed_empty_div_body_does_not_take_the_fold() {
    assert_eq!(
        html(":: t\n:  :::\n   :::\nlazy\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <div>\n    </div>\n  </dd>\n</dl>\n<p>lazy</p>"
    );
    assert!(list_twin("- :::\n  :::\nlazy\n").ends_with("<p>lazy</p>"));
}

#[test]
fn a_line_block_body_does_not_take_the_fold() {
    assert_eq!(
        html(":: t\n:  ::: |\n   a\n   :::\nlazy\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <div class=\"line-block\">\n      <p>a</p>\n    </div>\n  </dd>\n</dl>\n<p>lazy</p>"
    );
    assert!(list_twin("- ::: |\n  a\n  :::\nlazy\n").ends_with("<p>lazy</p>"));
}

// ---------------------------------------------------------------------------
// CONTROLS. Every one of these passes on the UNFIXED engine, and each is a
// body that DOES hold something a flush-left line folds into - so a fix that
// closed the body indiscriminately fails here.
// ---------------------------------------------------------------------------

#[test]
fn control_an_ordinary_paragraph_body_still_takes_the_fold() {
    assert_eq!(
        html(":: t\n:  body\nlazy\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>body\nlazy</dd>\n</dl>"
    );
}

#[test]
fn control_a_heading_body_still_takes_the_fold() {
    // The exception, pinned again beside the rows it does not move with.
    // `heading_folds_lazy.rs` holds the nested spelling this repeats.
    assert_eq!(
        html(":: t\n:  # H\nlazy\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <h1 id=\"H\">H</h1>\n    <p>lazy</p>\n  </dd>\n</dl>"
    );
}

#[test]
fn control_a_bare_image_body_still_takes_the_fold() {
    // A bare image line is a block ONLY while nothing folds into it
    // (`image_is_block`), and the line that decides that is the very line this
    // rule is being asked about - which the body collected so far does not hold
    // yet. Reading the block off a body that stops one line early made this a
    // standalone image plus a top-level paragraph, where the list twin folds.
    assert_eq!(
        html(":: t\n:  ![a](i.png)\nlazy\n"),
        "<dl>\n  <dt>t</dt>\n  <dd><img src=\"i.png\" alt=\"a\">\nlazy</dd>\n</dl>"
    );
    assert_eq!(
        list_twin("- ![a](i.png)\nlazy\n"),
        "<ul>\n  <li><img src=\"i.png\" alt=\"a\">\nlazy</li>\n</ul>"
    );
}

#[test]
fn control_a_captioned_image_body_still_takes_the_fold() {
    // Same reason: the caption is an inline continuation that a following
    // flush-left line extends, and the list twin says so too.
    assert_eq!(
        html(":: t\n:  ![a](i.png)\n   ^ cap\nlazy\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <figure>\n      <img src=\"i.png\" alt=\"a\">\n      <figcaption>cap\nlazy</figcaption>\n    </figure>\n  </dd>\n</dl>"
    );
}

#[test]
fn control_an_unterminated_div_body_still_takes_the_fold() {
    // An unterminated container holds an open paragraph, so the line folds INTO
    // it (carve#939, carve#980). This is the other side of the closed-div row
    // above and the two must not move together.
    assert_eq!(
        html(":: t\n:  ::: note\n   body\nlazy\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <aside class=\"admonition note\">\n      <p>body\nlazy</p>\n    </aside>\n  </dd>\n</dl>"
    );
}

#[test]
fn control_a_nested_list_body_still_takes_the_fold() {
    assert_eq!(
        html(":: t\n:  - x\nlazy\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <ul>\n      <li>x\nlazy</li>\n    </ul>\n  </dd>\n</dl>"
    );
}

#[test]
fn control_the_fence_halves_are_unchanged() {
    // #785 and #789 answered these, and the guard that answers them runs BEFORE
    // the new one - it has to, because an unterminated fence has no closer in
    // the body collected so far and PART 9 S10 degrades it to a paragraph, so
    // the new test alone would answer the open half backwards.
    assert_eq!(
        html(":: t\n:  ```\n   b\nlazy\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <pre><code>b\n</code></pre>\n  </dd>\n</dl>\n<p>lazy</p>"
    );
    assert_eq!(
        html(":: t\n:  ```\n   b\n   ```\nlazy\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <pre><code>b\n</code></pre>\n  </dd>\n</dl>\n<p>lazy</p>"
    );
}

#[test]
fn control_content_after_a_closed_block_opens_a_paragraph_the_fold_reaches() {
    // The latching failure. A body that finished a block and then collected
    // more content at its own column holds an open paragraph again.
    assert_eq!(
        html(":: t\n:  ---\n   more\nlazy\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <hr>\n    <p>more\nlazy</p>\n  </dd>\n</dl>"
    );
}
