//! Three shapes carve-rs#511 lists, each with the same shape of cause: a rule
//! this engine applies everywhere else, missing in one place.

use carve::to_html;

// --- A colon fence on a marker line opens (item 4) ---

#[test]
fn a_bare_colon_fence_on_a_marker_line_opens_a_div() {
    // An opener OPENS, closer or no closer (carve#514), and an empty body is a
    // container with nothing in it (carve#570). This kept `:::` as literal item
    // text unless item-owned content followed it.
    assert_eq!(
        to_html("- :::"),
        "<ul>\n  <li>\n    <div>\n    </div>\n  </li>\n</ul>"
    );
}

#[test]
fn a_sibling_marker_after_it_does_not_close_the_item_first() {
    assert_eq!(
        to_html("- :::\n- b"),
        "<ul>\n  <li>\n    <div>\n    </div>\n  </li>\n  <li>b</li>\n</ul>"
    );
}

#[test]
fn a_blank_or_a_flush_left_opener_leaves_the_fence_standing() {
    assert_eq!(
        to_html("- :::\n\nx"),
        "<ul>\n  <li>\n    <div>\n    </div>\n  </li>\n</ul>\n<p>x</p>"
    );
    assert_eq!(
        to_html("- :::\n# H"),
        "<ul>\n  <li>\n    <div>\n    </div>\n  </li>\n</ul>\n<section id=\"H\">\n  <h1>H</h1>\n</section>"
    );
}

#[test]
fn a_below_column_line_still_folds_the_fence_in_as_text() {
    // The strict content-column rule: `x` is lazy item text, and it takes the
    // fence with it.
    assert_eq!(to_html("- :::\nx"), "<ul>\n  <li>:::\nx</li>\n</ul>");
    assert_eq!(
        to_html("- ::: note\n :::"),
        "<ul>\n  <li>::: note\n:::</li>\n</ul>"
    );
}

#[test]
fn item_owned_content_still_becomes_the_body() {
    assert_eq!(
        to_html("- :::\n  x"),
        "<ul>\n  <li>\n    <div>\n      <p>x</p>\n    </div>\n  </li>\n</ul>"
    );
}

// --- A floating attribute skips an abbreviation definition (item 2) ---

#[test]
fn a_floating_attribute_skips_an_abbreviation_definition() {
    // §15 A2a: it attaches to the next VISIBLE block. The definition produced a
    // node, so the pending attributes were taken and then dropped - the other
    // invisible kinds never reach that path, which is why only this one lost
    // them.
    assert_eq!(to_html("{#i}\n*[A]: b\n\ne"), "<p id=\"i\">e</p>");
}

#[test]
fn the_other_invisible_kinds_were_already_skipped() {
    assert_eq!(to_html("{#i}\n%% c\n\ne"), "<p id=\"i\">e</p>");
    assert_eq!(to_html("{#i}\n[r]: /u\n\ne"), "<p id=\"i\">e</p>");
    assert_eq!(to_html("{#i}\n[^f]: n\n\ne"), "<p id=\"i\">e</p>");
}
