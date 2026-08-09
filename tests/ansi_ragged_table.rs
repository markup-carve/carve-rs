//! ANSI tables are display grids: unlike Markdown source, their box decoration
//! must reach the table's full width even when an AST row has fewer cells.

fn strip_ansi(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        for c in chars.by_ref() {
            if c == 'm' {
                break;
            }
        }
    }
    out
}

#[test]
fn short_header_row_is_padded_to_the_box_width() {
    let out = strip_ansi(&carve::to_ansi("| h |\n|---|\n| |x |\n"));

    assert!(out.contains("│ h │   │\n"));
    assert!(out.contains("┌───┬───┐"));
}

#[test]
fn short_body_row_is_padded_to_the_box_width() {
    let out = strip_ansi(&carve::to_ansi("| |x |\n|---|\n| y |\n"));

    assert!(out.contains("│ y │   │\n"));
    assert!(out.contains("└───┴───┘"));
}
