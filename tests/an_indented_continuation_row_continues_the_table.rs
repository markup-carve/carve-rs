//! A `+` row indented past the table's column is still that table's row (corpus 797).

fn cell(source: &str, text: &str) {
    let html = carve::to_html(source);
    assert!(
        html.contains(&format!("<td>{text}</td>")),
        "{source:?}: {html}"
    );
    assert!(!html.contains("<p>"), "{source:?}: {html}");
}

#[test]
fn any_indent_joins_the_row_above() {
    for indent in [" ", "  ", "   ", "\t"] {
        cell(&format!("| a |\n{indent}+ b |\n"), "a b");
    }
    cell("- | a |\n    + b |\n", "a b");
    cell("> | a |\n>  + b |\n", "a b");
}

#[test]
fn a_row_without_a_closing_pipe_stays_text() {
    let html = carve::to_html("| a |\n  + b\n");
    assert!(html.contains("<p>+ b</p>"), "{html}");
}
