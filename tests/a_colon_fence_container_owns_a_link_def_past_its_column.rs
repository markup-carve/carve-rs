//! A `:::` colon-fence container in a LIST-ITEM host owns a link definition
//! written past its content column - it is the container's text, not a hoisted
//! definition (markup-carve/carve corpus 451). A DEFINITION-LIST body still
//! hoists it, and a definition AT the fence's column registers in either host.
//!
//! ORACLE: the executable spec at spec main `1b27b68`; corpus section 451 pins
//! that a list-item host owns the line while a description host does not.

use carve::to_html;

fn hoisted(src: &str) -> bool {
    // The reference resolves only when the definition was hoisted.
    to_html(src).contains("href=\"/url\"")
}

/// LIST-ITEM host: a link def past the admonition's column is the admonition's
/// text, so `[r][]` stays literal.
#[test]
fn a_list_item_host_owns_the_link_def_past_the_fence_column() {
    assert!(!hoisted("- a\n  ::: note\n   [r]: /url\n  :::\n\n[r][]\n"));
}

/// A definition AT the fence's own column still registers (it is the container's
/// content, not past it).
#[test]
fn a_definition_at_the_fence_column_still_registers() {
    assert!(hoisted("- a\n  ::: note\n  [r]: /url\n  :::\n\n[r][]\n"));
}

/// A DEFINITION-LIST body does NOT own it: the description host hoists the same
/// past-the-column definition.
#[test]
fn a_description_body_still_hoists_the_link_def() {
    assert!(hoisted(
        ":: t\n:  a\n   ::: note\n    [r]: /url\n   :::\n\n[r][]\n"
    ));
}

/// A plain list item with no colon fence still absorbs an authored-base residual
/// and hoists a definition past its own column (carve#1705) - the ownership is
/// the colon fence's, not the item's.
#[test]
fn a_plain_item_without_a_fence_still_hoists_past_its_column() {
    assert!(hoisted("- a\n   [r]: /url\n\n[r][]\n"));
}
