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
//! The SUB-LIST lead is deliberately not covered here: carve-php agrees with
//! this engine that it stays tight and carve-js does not, so it is a live
//! question rather than a defect (reported on the issue).

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
}
