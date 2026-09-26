//! markup-carve/carve#2372: a pipe-table row is one line, so a comment holding
//! a line break has no spelling in a cell.

use carve::{html_to_carve, HtmlImportOptions};

fn import(html: &str) -> (String, Vec<(String, String)>) {
    let result = html_to_carve(html, &HtmlImportOptions::default()).unwrap();
    let rows = result
        .report
        .diagnostics
        .iter()
        .map(|d| {
            (
                d.code.as_str().to_string(),
                d.path.clone().unwrap_or_default(),
            )
        })
        .collect();
    (result.value, rows)
}

#[test]
fn a_multi_line_comment_in_a_cell_is_dropped_with_a_row() {
    for (html, carve, path) in [
        (
            "<table><tr><td>a <!-- x\ny --> b</td></tr></table>",
            "| a  b |\n",
            "/table[1]/tr[1]/td[1]/comment()[2]",
        ),
        (
            "<table><tr><td><p>a</p>\n\n<!-- note\n|x\n --></td></tr></table>",
            "| a |\n",
            "/table[1]/tr[1]/td[1]/comment()[3]",
        ),
    ] {
        let (value, rows) = import(html);
        assert_eq!(value, carve);
        let dropped: Vec<_> = rows.iter().filter(|r| r.0 == "element-dropped").collect();
        assert_eq!(dropped.len(), 1);
        assert_eq!(dropped[0].1, path);
    }
}

#[test]
fn a_one_line_comment_or_one_outside_a_cell_is_kept() {
    let (value, rows) = import("<table><tr><td>a <!-- one line --> b</td></tr></table>");
    assert_eq!(value, "| a {%  one line  %} b |\n");
    assert!(rows.is_empty());
    assert_eq!(import("<p>a <!-- x\ny --> b</p>").0, "a {%  x\ny  %} b\n");
}
