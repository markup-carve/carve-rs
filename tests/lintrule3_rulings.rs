//! The maintainer rulings of carve#6191 to #6196 (20 November).
use carve::lint::{LintOptions, lint};

fn ranges(source: &str, code: &str) -> Vec<(u32, u32)> {
    lint(source, &LintOptions::default())
        .iter()
        .filter(|finding| finding.code == code)
        .filter_map(|finding| finding.pos)
        .map(|pos| (pos.start_offset, pos.end_offset))
        .collect()
}

fn message(source: &str, code: &str) -> String {
    lint(source, &LintOptions::default())
        .into_iter()
        .find(|finding| finding.code == code)
        .map(|finding| finding.message)
        .unwrap_or_default()
}

#[test]
fn quote_marker_range_is_the_line_without_trailing_blanks() {
    let code = "blockquote-marker-without-space";
    assert_eq!(ranges(">a\n", code), [(0, 2)]);
    assert_eq!(ranges(">a  \n", code), [(0, 2)]);
    assert_eq!(ranges("---\ntitle: x\n>a  \n---\n", code), [(13, 15)]);
    // A no-break space is no space or tab, so it stays in the range (byte offsets).
    assert_eq!(ranges(">a\u{a0}\n", code), [(0, 4)]);
}

#[test]
fn overindented_opener_range_is_the_whole_opening_mark() {
    let code = "block-opener-overindented";
    assert_eq!(ranges("# H\n\n   ::::\n   x\n   ::::\n", code), [(8, 12)]);
    assert_eq!(ranges("   ### H\n", code), [(3, 6)]);
    assert_eq!(ranges("   ````\n   x\n   ````\n", code), [(3, 7)]);
    assert_eq!(ranges("   ***\n", code), [(3, 6)]);
    assert_eq!(ranges("   > q\n", code), [(3, 4)]);
    assert_eq!(ranges("   | a |\n", code), [(3, 4)]);
    assert_eq!(
        ranges("   {.x}\n   :::: foo\n   x\n   ::::\n", code),
        [(11, 15)]
    );
    assert_eq!(
        ranges("   {.x}\n    :::: foo\n    x\n    ::::\n", code),
        [(12, 16)]
    );
    assert_eq!(
        ranges("    {.x}\n   :::: foo\n   x\n   ::::\n", code),
        [(12, 16)]
    );
}

#[test]
fn include_line_is_no_block_opener_in_an_item() {
    assert!(ranges("- a\n\n      {{ x.crv }}\n", "list-item-block-overindented").is_empty());
}

#[test]
fn colon_run_in_a_quote_or_item_inside_a_div_is_not_compared() {
    let code = "colon-fence-length-mismatch";
    assert_eq!(ranges(":::: x\n:::::  \n", code), [(7, 12)]);
    assert!(ranges("::::\n> :::\n", code).is_empty());
    assert!(ranges("::::\n- a\n\n  :::  \n", code).is_empty());
    assert_eq!(ranges("> ::::\n> :::\n", code), [(9, 12)]);
}

#[test]
fn attribute_shaped_paragraph_line_gets_only_the_blank_line_warning() {
    assert!(ranges("para\n{.c}\n", "unattached-block-attribute").is_empty());
    assert_eq!(
        ranges("para\n{.c}\n", "block-opener-without-blank-line"),
        [(5, 9)]
    );
}

#[test]
fn quote_line_is_blank_inside_its_quote() {
    let code = "unattached-block-attribute";
    assert!(
        message("> {.c}\n>\n> # H\n", code).starts_with("A blank line follows this attribute line")
    );
    assert!(
        message("- > {.c}\n  >\n", code).starts_with("A blank line follows this attribute line")
    );
    assert!(ranges("{.c}\n>>\n", code).is_empty());
    assert!(message("> > {.c}\n>\n", code).starts_with("This attribute line reaches no block"));
}

#[test]
fn every_attribute_line_spelling_counts_for_both_attribute_rules() {
    let opener = "block-opener-without-blank-line";
    assert_eq!(ranges("para\n{ .c }\n", opener), [(5, 11)]);
    assert_eq!(ranges("para\n{#id}\n", opener), [(5, 10)]);
    assert_eq!(ranges("{ #id }\n", "unattached-block-attribute"), [(0, 7)]);
}
