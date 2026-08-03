//! A blank line between an item's blocks makes the list LOOSE, whatever the
//! item's first content happens to be.
//!
//! Three marker-line leads take their own branch in the list parser - a
//! standalone attribute block, a block quote, and a sub-list - and each built
//! its item and `continue`d without ever consulting the looseness rule the
//! normal path applies. So `- {a=b}` / `x` / blank / `Body.` stayed tight while
//! `- x` / blank / `Body.` went loose, on the same blank line (carve-rs#476).
//!
//! carve-js and carve-php render both as loose.
//!
//! The SUB-LIST lead was left open by that fix, since carve-php agreed with this
//! engine that it stays tight and carve-js did not. carve-js was right - PART 9
//! §11 loosens on the blank between an item's blocks, and the lead being a marker
//! line is not part of that test - so both engines were fixed (carve-rs#490,
//! carve-php#681) and the case is covered here.

use carve::to_html;

fn is_loose(source: &str) -> bool {
    to_html(source).contains("<p>Body.</p>")
}

#[test]
fn an_attribute_block_lead_still_loosens() {
    assert!(is_loose("- {a=b}\n  x\n\n  Body.\n"));
}

#[test]
fn an_attributed_heading_lead_still_loosens() {
    // The shape the issue reported.
    let html = to_html("- {a=b .c}\n  # H\n\n  Body.\n");

    assert!(html.contains("<p>Body.</p>"), "{html}");
    assert!(
        html.contains("<h1 a=\"b\" class=\"c\" id=\"H\">H</h1>"),
        "{html}"
    );
}

#[test]
fn a_block_quote_lead_still_loosens() {
    assert!(is_loose("- > q\n\n  Body.\n"));
}

#[test]
fn a_sub_list_lead_still_loosens_at_the_outer_content_column() {
    assert!(is_loose("- - a\n\n  Body.\n"));
}

#[test]
fn a_sub_list_lead_does_not_take_the_inner_lists_looseness() {
    // At the INNER item's content column the body belongs to the sub-list, which
    // loosens on its own; looseness does not propagate outwards (§17, corpus 142).
    let html = to_html("- - a\n\n    Body.\n");

    assert_eq!(
        html.trim(),
        "<ul>\n  <li>\n    <ul>\n      <li><p>a</p>\n        <p>Body.</p>\n      </li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn a_blank_ends_the_sub_lists_lazy_continuation() {
    // Flush-left text after the blank is a new top-level block: the blank closed
    // the inner item's paragraph, so there is nothing left to lazily continue.
    let html = to_html("- - a\n\nBody.\n");

    assert_eq!(
        html.trim(),
        "<ul>\n  <li>\n    <ul>\n      <li>a</li>\n    </ul>\n  </li>\n</ul>\n<p>Body.</p>"
    );
}

#[test]
fn flush_left_text_still_folds_without_a_blank() {
    assert_eq!(
        to_html("- - a\nlazy\n").trim(),
        "<ul>\n  <li>\n    <ul>\n      <li>a\nlazy</li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn the_ordinary_leads_are_unchanged() {
    assert!(is_loose("- x\n\n  Body.\n"));
    assert!(is_loose("- # H\n\n  Body.\n"));
}

#[test]
fn no_blank_line_stays_tight() {
    // The guard must loosen on the BLANK, not on the lead kind.
    assert!(!is_loose("- {a=b}\n  x\n  Body.\n"));
    assert!(!is_loose("- > q\n  Body.\n"));
    assert!(!is_loose("- x\n  Body.\n"));
    assert!(!is_loose("- - a\n  Body.\n"));
}
