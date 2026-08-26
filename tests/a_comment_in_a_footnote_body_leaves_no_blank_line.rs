//! A comment is INVISIBLE, so it may not produce output that the visible
//! construct it stands in for does not (markup-carve/carve-rs#1439).
//!
//! A `%%` comment written inside a footnote body left an empty line in the
//! rendered `li` where the comment had been. The control - the same document
//! with a real blank line in place of the comment - emits no empty line, so an
//! invisible construct had an effect the visible one lacked, which inverts
//! section 24 C3 and L1b. markup-carve/carve#665 already ruled the sibling
//! case, where an engine "left a blank line inside the item where the attached
//! definition used to be".
//!
//! The bytes matter even though HTML collapses inter-block whitespace: the
//! spec corpus compares output byte for byte, so the row for this shape could
//! not be written while it stood. carve-php was correct here; carve-js held
//! the same defect and was fixed alongside (markup-carve/carve-js#1545).
//!
//! Every assertion runs on BOTH render entry points. `to_html` opens with a
//! layout fast path and the CLI goes through `try_to_html_with_options` with
//! its transforms, and a defect living on only one of the two is exactly what
//! a single-path test cannot see.

const COMMENTED: &str = "[^b]: para\n      %% c\n      more\n\nuse[^b]\n";
const CONTROL: &str = "[^b]: para\n\n      more\n\nuse[^b]\n";

fn both(source: &str) -> (String, String) {
    let convenience = carve::to_html(source);
    let cli = carve::try_to_html_with_options(source, &carve::Options::default())
        .expect("the default profile denies nothing");
    (convenience, cli)
}

#[test]
fn the_commented_body_renders_exactly_like_the_blank_line_control() {
    let (commented, commented_cli) = both(COMMENTED);
    let (control, control_cli) = both(CONTROL);
    assert_eq!(commented, control);
    assert_eq!(commented_cli, control_cli);
}

#[test]
fn no_empty_line_sits_between_the_two_paragraphs_of_the_note() {
    let (commented, commented_cli) = both(COMMENTED);
    for html in [&commented, &commented_cli] {
        assert!(
            !html.contains("\n\n"),
            "an empty line survived in the endnote:\n{html}"
        );
        assert!(
            html.contains("<p>para</p>\n      <p>more"),
            "the two paragraphs are not adjacent:\n{html}"
        );
    }
}

#[test]
fn a_body_that_is_only_a_comment_still_carries_its_backlink() {
    // The filter that drops the invisible block must not drop the note's way
    // back with it - this is the arm where every block of the body is
    // invisible, and it is what a naive filter breaks.
    let (only, only_cli) = both("[^b]: %% only\n\nuse[^b]\n");
    for html in [&only, &only_cli] {
        assert!(
            html.contains("role=\"doc-backlink\""),
            "the endnote lost its backlink:\n{html}"
        );
        assert!(!html.contains("\n\n"), "an empty line survived:\n{html}");
    }
}

#[test]
fn control_a_comment_between_two_top_level_paragraphs_also_leaves_none() {
    // This already passed, and pins that the fix is not a blanket whitespace
    // squeeze - it is what tells a real regression from this one.
    let (top, top_cli) = both("a\n%% c\n\nb\n");
    assert_eq!(top, "<p>a</p>\n<p>b</p>");
    assert_eq!(top_cli, "<p>a</p>\n<p>b</p>");
}
