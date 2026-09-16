//! A table row whose every cell is blank is not a table row
//! (markup-carve/carve#1954), so the writer refuses it and the HTML importer
//! drops it, keeping the rest of the table (ruling markup-carve/carve-js#1822,
//! markup-carve/carve-rs#1735).

use carve::{
    html_to_ast, html_to_carve, parse, render_carve, BlockNode, Document, HtmlImportDiagnosticCode,
    HtmlImportOptions, RenderCarveError,
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

fn refused(doc: &Document) -> bool {
    match render_carve(doc) {
        Err(RenderCarveError::SourceUnspellable(error)) => error.node_type() == "table_row",
        _ => false,
    }
}

#[test]
fn the_writer_refuses_a_blank_data_row() {
    assert!(refused(&blanked("| a |\n| b |\n", 1)));
}

#[test]
fn the_writer_refuses_a_blank_header_row() {
    assert!(refused(&blanked("|= h |\n| b |\n", 0)));
}

#[test]
fn the_writer_refuses_a_blank_row_of_several_cells() {
    assert!(refused(&blanked("| a | b |\n| c | d |\n", 0)));
}

#[test]
fn a_row_attribute_does_not_save_it() {
    assert!(refused(&blanked("| a |{.r}\n| b |\n", 0)));
}

/// Control: a cell carrying anything but content is not blank.
#[test]
fn the_control_rows_are_written() {
    let source = "| a | |\n|{.x} |\n|> |\n| < |\n";
    assert_eq!(render_carve(&parse(source)).unwrap(), source);
}

macro_rules! drops {
    ($($name:ident: $html:expr => $carve:expr, [$($path:expr),*],)*) => {
        $(
            #[test]
            fn $name() {
                let (carve, rows) = imported($html);
                assert_eq!(carve, $carve);
                assert_eq!(
                    rows,
                    vec![$((HtmlImportDiagnosticCode::StructureUnspellable, $path.to_string())),*]
                );
            }
        )*
    };
}

drops! {
    a_blank_header_row: "<table><thead><tr><th></th></tr></thead><tbody><tr><td>a</td></tr></tbody></table>"
        => "| a |\n", ["/table[1]/tr[1]"],
    a_blank_data_row: "<table><tr><td></td></tr><tr><td>a</td></tr></table>"
        => "| a |\n", ["/table[1]/tr[1]"],
    several_columns: "<table><tr><td></td><td></td></tr><tr><td>a</td><td>b</td></tr></table>"
        => "| a | b |\n", ["/table[1]/tr[1]"],
    between_two_filled_rows: "<table><tr><td>a</td></tr><tr><td></td></tr><tr><td>b</td></tr></table>"
        => "| a |\n| b |\n", ["/table[1]/tr[2]"],
}

#[test]
fn a_table_with_no_row_left_goes_with_its_caption() {
    let (carve, rows) = imported("<table><caption>c</caption><tr><td></td></tr></table>");
    assert_eq!(carve, "\n");
    assert_eq!(
        rows,
        vec![
            (
                HtmlImportDiagnosticCode::StructureUnspellable,
                "/table[1]/tr[1]".to_string()
            ),
            (
                HtmlImportDiagnosticCode::ElementDropped,
                "/table[1]/tr[1]".to_string()
            ),
        ]
    );
}

#[test]
fn a_caption_whose_table_keeps_a_row_stays() {
    let (carve, _) =
        imported("<table><caption>c</caption><tr><td></td></tr><tr><td>a</td></tr></table>");
    assert_eq!(carve, "| a |\n^ c\n");
}

/// Control: a cell carrying an attribute is not blank, and keeps its row.
#[test]
fn an_attributed_empty_cell_keeps_its_row() {
    let (carve, rows) =
        imported("<table><tr><td class=\"x\"></td></tr><tr><td>a</td></tr></table>");
    assert_eq!(carve, "|{.x} |\n| a |\n");
    assert_eq!(rows, Vec::new());
}

/// The AST exit keeps the row: the tree renders it, and only a writer has no
/// spelling for it.
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
