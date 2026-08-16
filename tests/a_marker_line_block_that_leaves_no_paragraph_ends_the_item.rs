//! NO OPEN PARAGRAPH, NO LAZY LINE, in a list item as in a block quote
//! (PART 1 S4, ruled in markup-carve/carve#1280).
//!
//! Lazy continuation extends an OPEN PARAGRAPH and nothing else. The clause
//! names no container, and a block that leaves no paragraph open leaves none
//! wherever it was written - so `- # H` writes a heading as the item's first
//! block exactly as `- ` plus an indented `# H` would, and a flush-left line
//! after it ends the item.
//!
//! Every engine folded eight shapes in a list item while ending all eight in a
//! block quote, on one rule that describes one container and not the other:
//! heading, table, thematic break, line comment, comment fence, link reference
//! definition, footnote definition, attribute block. `> # H` / `tail` already
//! ended the quote in carve-rs, carve-js and carve-php alike; these are the
//! same derivation with the same answer.
//!
//! DELIBERATELY OUT OF SCOPE, and asserted as such below: the same shapes at
//! the item's CONTENT COLUMN. Extending the rule there moves corpus
//! 75-list-nesting-and-looseness-4 to a third answer nobody has agreed to, so
//! the clause leaves that half open and the controls at the bottom of this file
//! pin it where it is.

fn html(src: &str) -> String {
    carve::to_html(src)
}

// ---------------------------------------------------------------------------
// The eight kinds. Each is `<marker> <block>` on one line, a flush-left line
// under it, and the item ending before that line.
// ---------------------------------------------------------------------------

#[test]
fn a_heading_ends_the_item() {
    assert_eq!(
        html("- # H\ntail\n"),
        "<ul>\n  <li>\n    <h1 id=\"H\">H</h1>\n  </li>\n</ul>\n<p>tail</p>"
    );
}

#[test]
fn the_block_quote_spelling_is_the_same_document() {
    // The pair that says the two containers are one rule. This one already
    // answered this way in every engine, which is what made the item's answer
    // an inconsistency rather than a design.
    assert_eq!(
        html("> # H\ntail\n"),
        "<blockquote>\n  <h1 id=\"H\">H</h1>\n</blockquote>\n<p>tail</p>"
    );
}

#[test]
fn a_table_ends_the_item() {
    assert_eq!(
        html("- | a | b |\ntail\n"),
        "<ul>\n  <li>\n    <table>\n      <tbody>\n        <tr><td>a</td><td>b</td></tr>\n      </tbody>\n    </table>\n  </li>\n</ul>\n<p>tail</p>"
    );
}

#[test]
fn a_thematic_break_ends_the_item() {
    assert_eq!(
        html("- ---\ntail\n"),
        "<ul>\n  <li>\n    <hr>\n  </li>\n</ul>\n<p>tail</p>"
    );
}

#[test]
fn a_line_comment_ends_the_item() {
    // Invisible is not open. The item renders EMPTY rather than absorbing the
    // line below it, which is the whole difference: a comment closes nothing,
    // and it opens nothing either.
    assert_eq!(
        html("- %% c\ntail\n"),
        "<ul>\n  <li></li>\n</ul>\n<p>tail</p>"
    );
}

#[test]
fn a_comment_fence_ends_the_item() {
    // The fence spelling answers the same way. Its closer travels with its
    // opener - the item holds an empty comment - and everything below re-parses
    // at document level, the derivation `- ``` ` already got.
    assert_eq!(
        html("- %%%\nc\n%%%\ntail\n"),
        "<ul>\n  <li></li>\n</ul>\n<p>c</p>\n<p>tail</p>"
    );
}

#[test]
fn a_link_reference_definition_ends_the_item() {
    // §17 L6: ending the item disposes of the line BELOW the definition, never
    // of the definition, which is collected from wherever it was written. The
    // use below still resolves.
    assert_eq!(
        html("- [r]: /u\ntail\n\n[r][]\n"),
        "<ul>\n  <li></li>\n</ul>\n<p>tail</p>\n<p><a href=\"/u\">r</a></p>"
    );
}

#[test]
fn a_footnote_definition_ends_the_item() {
    let out = html("- [^f]: t\ntail\n\nsee[^f]\n");

    assert!(
        out.starts_with("<ul>\n  <li></li>\n</ul>\n<p>tail</p>\n"),
        "{out}"
    );
    // Collected from inside the item it no longer holds, and still resolved.
    assert!(
        out.contains("<li id=\"fn1\">\n      <p>t<a href=\"#fnref1\""),
        "{out}"
    );
}

#[test]
fn an_attribute_block_ends_the_item() {
    // An attribute line opens no paragraph either, so it never reaches the line
    // below: the item ends first and the attribute is left unconsumed. It is
    // scoped to the item and does not travel out of it (§15 A4,
    // markup-carve/carve#1281).
    assert_eq!(
        html("- {.k}\ntail\n"),
        "<ul>\n  <li></li>\n</ul>\n<p>tail</p>"
    );
    // The same attribute, with a BLOCK below it at column 0 rather than prose.
    // It used to pull that block into the item to have something to attribute.
    assert_eq!(
        html("- {.k}\n# H\n"),
        "<ul>\n  <li></li>\n</ul>\n<section id=\"H\">\n  <h1>H</h1>\n</section>"
    );
    assert_eq!(
        html("- {.k}\n| a | b |\n"),
        "<ul>\n  <li></li>\n</ul>\n<table>\n  <tbody>\n    <tr><td>a</td><td>b</td></tr>\n  </tbody>\n</table>"
    );
}

// ---------------------------------------------------------------------------
// The question is asked RECURSIVELY, and depth is not a parameter.
// ---------------------------------------------------------------------------

#[test]
fn a_nested_quote_answers_with_its_own_last_block() {
    // Non-emptiness is not the test. A quote is a container like any other and
    // what decides is the block IT ends on.
    assert_eq!(
        html("- > # H\ntail\n"),
        "<ul>\n  <li>\n    <blockquote>\n      <h1 id=\"H\">H</h1>\n    </blockquote>\n  </li>\n</ul>\n<p>tail</p>"
    );
}

#[test]
fn the_same_answer_one_level_down() {
    // carve-rs#1025: this engine folded at depth one and ended at depth two, on
    // a question that has nothing to do with how many items wrap the heading.
    // Depth two was the right answer, so depth one moved to meet it - and the
    // two marker-line spellings of the nested document now agree.
    assert_eq!(
        html("- - # H\ntail\n"),
        "<ul>\n  <li>\n    <ul>\n      <li>\n        <h1 id=\"H\">H</h1>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>tail</p>"
    );
    assert_eq!(
        html("- a\n  - # N\nlazy\n"),
        "<ul>\n  <li>a\n    <ul>\n      <li>\n        <h1 id=\"N\">N</h1>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>lazy</p>"
    );
}

// ---------------------------------------------------------------------------
// CONTROLS. The rule has ONE parameter, so the controls are the documents where
// that parameter has the other value - and the half the clause leaves open.
// ---------------------------------------------------------------------------

#[test]
fn control_a_nested_quote_with_an_open_paragraph_still_folds() {
    assert_eq!(
        html("- > q\ntail\n"),
        "<ul>\n  <li>\n    <blockquote><p>q\ntail</p></blockquote>\n  </li>\n</ul>"
    );
}

#[test]
fn control_plain_lead_text_is_ordinary_lazy_continuation() {
    assert_eq!(html("- a\ntail\n"), "<ul>\n  <li>a\ntail</li>\n</ul>");
}

#[test]
fn control_a_sibling_marker_still_opens_a_sibling() {
    // "The item ended" rather than "the item swallowed something": the list is
    // still open for its next marker.
    assert_eq!(
        html("- # H\n- next\n"),
        "<ul>\n  <li>\n    <h1 id=\"H\">H</h1>\n  </li>\n  <li>next</li>\n</ul>"
    );
}

#[test]
fn control_the_content_column_half_is_untouched() {
    // OUT OF SCOPE, deliberately. The same heading one column further in still
    // folds, and corpus 75-list-nesting-and-looseness-4 still reads as it did.
    // A fix that reaches this document has overshot the ruling.
    assert_eq!(
        html("- a\n  # H\ntail\n"),
        "<ul>\n  <li>a\n    <h1 id=\"H\">H</h1>\n    tail\n  </li>\n</ul>"
    );
    assert_eq!(
        html("- a\n  - b\n    # N\nlazy\n"),
        "<ul>\n  <li>a\n    <ul>\n      <li>b\n        <h1 id=\"N\">N</h1>\n        lazy\n      </li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn control_the_wrapped_attribute_block_is_untouched() {
    // The other half the clause names as open: `{.a` opens no paragraph, but
    // its continuation line arrives at the CONTENT COLUMN and is what reopens
    // one. It stays literal text here, as it was.
    assert_eq!(
        html("- {.a\n  .b}\ntail\n"),
        "<ul>\n  <li>{.a\n.b}\ntail</li>\n</ul>"
    );
}

#[test]
fn control_an_indented_attribute_still_attaches_inside_the_item() {
    // Scoped is not disabled. At the item's content column the attribute
    // reaches a block that is genuinely the item's, and it applies (corpus 170).
    assert_eq!(
        html("- {a=b .c}\n  # H\n"),
        "<ul>\n  <li>\n    <h1 a=\"b\" class=\"c\" id=\"H\">H</h1>\n  </li>\n</ul>"
    );
}

#[test]
fn control_the_two_rows_that_already_ended_are_unchanged() {
    // A closed code fence and a bare div ended the item before this ruling and
    // were the unexplained exceptions to whatever rule produced the rest. They
    // are now the two ordinary cases.
    assert!(html("- ```\nx\n```\ntail\n").contains("</code></pre>\n  </li>\n</ul>"));
    assert!(html("- :::\n  :::\ntail\n").contains("</div>\n  </li>\n</ul>\n<p>tail</p>"));
}
