//! A table cell is one line, so a hard break in one is written as one space and
//! the HTML importer reports it (ruling markup-carve/carve#2067,
//! markup-carve/carve-rs#1714).

use carve::{
    html_to_ast, html_to_carve, render_carve, to_carve, to_html, to_json, HtmlImportDiagnosticCode,
    HtmlImportOptions,
};

fn imported(html: &str) -> (String, Vec<(HtmlImportDiagnosticCode, String)>) {
    let result = html_to_carve(html, &HtmlImportOptions::default()).unwrap();
    let rows = result
        .report
        .diagnostics
        .iter()
        .map(|d| (d.code, d.path.clone().unwrap_or_default()))
        .collect();
    (result.value, rows)
}

macro_rules! cases {
    ($($name:ident: $html:expr => $carve:expr, $rows:expr,)*) => {
        $(
            mod $name {
                use super::*;

                #[test]
                fn imports_as_the_expected_bytes() {
                    assert_eq!(imported($html).0, $carve);
                }

                #[test]
                fn the_import_keeps_the_table() {
                    assert!(to_html(&imported($html).0).starts_with("<table>"));
                }

                #[test]
                fn the_import_is_a_fixed_point_of_fmt() {
                    let carve = imported($html).0;
                    assert_eq!(to_carve(&carve), carve);
                }

                #[test]
                fn every_break_is_reported() {
                    let expected: Vec<(HtmlImportDiagnosticCode, String)> = $rows
                        .iter()
                        .map(|path: &&str| (HtmlImportDiagnosticCode::StructureUnspellable, path.to_string()))
                        .collect();
                    assert_eq!(imported($html).1, expected);
                }
            }
        )*
    };
}

cases! {
    between_words: "<table><tr><td>x<br>y</td></tr></table>" => "| x y |\n",
        ["/table[1]/tr[1]/td[1]/br[2]"],
    ending_a_middle_cell: "<table><tr><td>x<br></td><td>c</td></tr></table>" => "| x | c |\n",
        ["/table[1]/tr[1]/td[1]/br[2]"],
    at_both_edges: "<table><tr><td>a</td><td><br>x<br></td></tr></table>" => "| a | x |\n",
        ["/table[1]/tr[1]/td[2]/br[1]", "/table[1]/tr[1]/td[2]/br[3]"],
    alone: "<table><tr><td><br></td><td>c</td></tr></table>" => "| | c |\n",
        ["/table[1]/tr[1]/td[1]/br[1]"],
    inside_a_strong: "<table><tr><td><strong>x<br>y</strong></td></tr></table>" => "| *x y* |\n",
        ["/table[1]/tr[1]/td[1]/strong[1]/br[2]"],
}

/// Control: a break outside a table is still a backslash and a newline, with no row.
#[test]
fn a_paragraph_break_is_unchanged() {
    assert_eq!(imported("<p>x<br>y</p>"), ("x\\\ny\n".to_string(), vec![]));
}

/// The AST exit keeps the break: only a writer loses it.
#[test]
fn the_ast_import_keeps_the_break() {
    let doc = html_to_ast(
        "<table><tr><td>x<br>y</td></tr></table>",
        &HtmlImportOptions::default(),
    )
    .unwrap()
    .value;
    assert!(to_json(&doc).contains(r#"{"type":"hard_break"}"#));
    assert_eq!(render_carve(&doc).unwrap(), "| x y |\n");
}
