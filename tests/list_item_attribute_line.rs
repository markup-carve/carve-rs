//! Braces ALONE on a list-item marker line are a block-attribute line, not item
//! text (grammar PART 9 §15 A8, spec corpus 170, carve#454/#457).
//!
//! The discriminator is whether CONTENT FOLLOWS the braces, not the column they
//! sit in: a container does not get its own attribute rules. carve-rs was the
//! engine that read the brace-only form as literal text and dropped the
//! attributes, while carve-js, carve-php and the executable spec attached them.

#[test]
fn brace_only_marker_line_attributes_the_next_block() {
    assert_eq!(
        carve::to_html("- {a=b .c}\n  # Attributed heading\n"),
        "<ul>\n  <li>\n    <h1 a=\"b\" class=\"c\" id=\"Attributed-heading\">Attributed heading</h1>\n  </li>\n</ul>"
    );
}

#[test]
fn braces_trailed_by_text_stay_literal() {
    // Corpus 88-list-item-attributes-7: a space after the marker makes the
    // braces part of the text.
    assert_eq!(
        carve::to_html("- {.c} literal text\n"),
        "<ul>\n  <li>{.c} literal text</li>\n</ul>"
    );
}

#[test]
fn abutting_braces_attribute_the_item_itself() {
    assert_eq!(
        carve::to_html("-{.item} An attributed item.\n"),
        "<ul>\n  <li class=\"item\">An attributed item.</li>\n</ul>"
    );
}

#[test]
fn an_attributed_paragraph_in_a_tight_item_is_wrapped() {
    // A tight item renders its paragraphs bare, but attributes have nowhere to
    // go without the `<p>` - rendering it bare silently dropped the class.
    assert_eq!(
        carve::to_html("- {a=b .c}\n  text\n"),
        "<ul>\n  <li><p a=\"b\" class=\"c\">text</p></li>\n</ul>"
    );
}

#[test]
fn a_brace_only_marker_line_with_no_following_block_leaves_an_empty_item() {
    assert_eq!(carve::to_html("- {.c}\n"), "<ul>\n  <li></li>\n</ul>");
}

#[test]
fn a_multi_line_attribute_block_on_the_marker_line_attaches() {
    assert_eq!(
        carve::to_html("- {#id\n  .foo}\n  # H\n"),
        "<ul>\n  <li>\n    <h1 id=\"id\" class=\"foo\">H</h1>\n  </li>\n</ul>"
    );
}

#[test]
fn an_invalid_brace_run_keeps_its_lazy_line_in_the_item() {
    // The paragraph path collects lazy continuation; the block path does not.
    // Routing a brace run there on the guess that it is an attribute line made
    // `lazy` escape to a top-level paragraph.
    assert_eq!(
        carve::to_html("- {not attrs\nlazy\n"),
        "<ul>\n  <li>{not attrs\nlazy</li>\n</ul>"
    );
}

#[test]
fn an_invalid_brace_run_stays_literal() {
    assert_eq!(
        carve::to_html("- {not attrs\n  # H\n"),
        "<ul>\n  <li>{not attrs\n    <h1 id=\"H\">H</h1>\n  </li>\n</ul>"
    );
}
