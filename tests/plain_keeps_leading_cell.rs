//! A document-level trim must not reach into the first content line.
//!
//! The plain renderer trimmed newlines AND ASCII spaces from both document edges,
//! so a table whose first row starts with an EMPTY cell lost the leading space of
//! ` | b`. That space is the empty field: without it the line reads as a leading
//! pipe rather than as an empty cell followed by a separator, and the row silently
//! has one field instead of two (carve#352, corpus
//! 96-table-span-marker-in-first-column and 09-tables-7). carve-js and carve-php
//! both kept it.
//!
//! A leading tab from a code block survived this before only because the character
//! class happened to omit `\t`, so the same class of bug was one character away
//! from biting there too -- and did, in the other two engines (carve-js#424).

#[test]
fn an_empty_first_cell_keeps_its_column() {
    assert_eq!(
        carve::to_plain_text("| | b |\n| c | d |\n"),
        " | b\nc | d\n"
    );
}

#[test]
fn the_row_keeps_its_field_count() {
    // The point of the leading space: splitting the line on the separator yields
    // the same number of fields as the row below, with the first one empty. Plain
    // text does not pad cells, so the pipes do not line up -- that is not what is
    // being preserved here.
    let out = carve::to_plain_text("| | b |\n| c | d |\n");
    let lines: Vec<&str> = out.lines().collect();
    let fields = |line: &str| line.split(" | ").count();
    assert_eq!(
        fields(lines[0]),
        fields(lines[1]),
        "field count differs: {out:?}"
    );
    assert!(
        lines[0].starts_with(" | "),
        "empty first field lost: {out:?}"
    );
}

#[test]
fn a_leading_tab_in_code_still_survives() {
    assert_eq!(
        carve::to_plain_text("```\n\tindented with a tab\n```\n"),
        "\tindented with a tab\n"
    );
}

#[test]
fn trailing_whitespace_is_still_trimmed() {
    // At the end of the document that whitespace is layout, not content: a row
    // ending in an empty cell renders `x | ` and the space is a separator artifact.
    assert_eq!(
        carve::to_plain_text("|= A |= B |\n| x |  |\n"),
        "A | B\nx |\n"
    );
}

#[test]
fn blank_lines_around_the_document_are_still_dropped() {
    assert_eq!(carve::to_plain_text("\n\n\nhello\n\n\n"), "hello\n");
}
