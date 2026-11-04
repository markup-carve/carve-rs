//! A row of empty cells is a table row (CARVE-P9-082), so the writer spells it
//! with one slot per cell (CARVE-P11-014) and the HTML importer keeps it.

use carve::{
    html_to_ast, html_to_carve, parse, render_carve, BlockNode, Document, HtmlImportDiagnosticCode,
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

/// Parse `source`, a table, and empty every cell of row `row`.
fn blanked(source: &str, row: usize) -> Document {
    let mut doc = parse(source);
    let BlockNode::Table(table) = &mut doc.children[0] else {
        panic!("expected a table");
    };
    for cell in &mut table.rows[row].cells {
        cell.children.clear();
    }
    doc
}

/// The written source reads back as the same tree.
fn written(doc: &Document) -> String {
    let source = render_carve(doc).expect("a row of empty cells has a spelling");
    assert_eq!(
        carve::render_html(&parse(&source)).unwrap(),
        carve::render_html(doc).unwrap(),
        "{source:?}"
    );
    source
}

#[test]
fn a_blank_data_row_is_written() {
    assert_eq!(written(&blanked("| a |\n| b |\n", 1)), "| a |\n| |\n");
}

#[test]
fn a_blank_header_row_is_written() {
    assert_eq!(written(&blanked("|= h |\n| b |\n", 0)), "|= |\n| b |\n");
}

#[test]
fn a_blank_row_of_several_cells_is_written() {
    assert_eq!(
        written(&blanked("| a | b |\n| c | d |\n", 0)),
        "| | |\n| c | d |\n"
    );
}

#[test]
fn a_row_attribute_rides_on_a_blank_row() {
    assert_eq!(
        written(&blanked("| a |{.r}\n| b |\n", 0)),
        "| |{.r}\n| b |\n"
    );
}

#[test]
fn the_marked_rows_are_written() {
    let source = "| a | |\n|{.x} |\n|{>} |\n| < |\n";
    assert_eq!(render_carve(&parse(source)).unwrap(), source);
}

macro_rules! keeps {
    ($($name:ident: $html:expr => $carve:expr,)*) => {
        $(
            #[test]
            fn $name() {
                let (carve, rows) = imported($html);
                assert_eq!(carve, $carve);
                assert_eq!(rows, Vec::new());
            }
        )*
    };
}

keeps! {
    a_blank_header_row: "<table><thead><tr><th></th></tr></thead><tbody><tr><td>a</td></tr></tbody></table>"
        => "|= |\n| a |\n",
    a_blank_data_row: "<table><tr><td></td></tr><tr><td>a</td></tr></table>"
        => "| |\n| a |\n",
    several_columns: "<table><tr><td></td><td></td></tr><tr><td>a</td><td>b</td></tr></table>"
        => "| | |\n| a | b |\n",
    between_two_filled_rows: "<table><tr><td>a</td></tr><tr><td></td></tr><tr><td>b</td></tr></table>"
        => "| a |\n| |\n| b |\n",
    a_table_of_only_a_blank_row_keeps_its_caption: "<table><caption>c</caption><tr><td></td></tr></table>"
        => "| |\n^ c\n",
}

#[test]
fn the_ast_exit_keeps_the_row() {
    let doc = html_to_ast(
        "<table><tr><td></td></tr><tr><td>a</td></tr></table>",
        &HtmlImportOptions::default(),
    )
    .unwrap()
    .value;
    let BlockNode::Table(table) = &doc.children[0] else {
        panic!("expected a table");
    };
    assert_eq!(table.rows.len(), 2);
}

#[test]
fn a_blank_djot_row_imports_as_a_table() {
    let result = carve::migrate_djot("| |\n");
    assert_eq!(result.value, "| |\n");
    assert!(!result
        .report
        .diagnostics
        .iter()
        .any(|d| d.code == "structure-unspellable"));
}
