//! The list-table import option and the Markdown target's list-table writing
//! (markup-carve/carve#2391, markup-carve/carve#2392). The byte expectations
//! are the ones carve-js and carve-php produce for the same inputs.

use carve::{
    html_to_ast, html_to_carve, to_markdown, HtmlImportDiagnosticCode, HtmlImportOptions,
    HtmlImportSeverity,
};
use std::io::Write;
use std::process::{Command, Stdio};

const STEPS: &str = "<table>\n<caption>Steps</caption>\n<tr><th>Step</th><th>Detail</th></tr>\n\
<tr><th>1</th><td><p>Install.</p><pre><code>npm i</code></pre></td></tr>\n\
<tr><td colspan=\"2\">^</td></tr>\n</table>";

const STEPS_CARVE: &str = "{header-rows=1}\n::: list-table \"Steps\"\n- - Step\n  - Detail\n\
- -{header} 1\n\n  - Install.\n\n    ```\n    npm i\n    ```\n- - \\^\n  - <\n:::\n";

fn on() -> HtmlImportOptions {
    HtmlImportOptions {
        list_table_for_block_cells: true,
        ..Default::default()
    }
}

#[test]
fn the_contract_example_imports_as_a_list_table() {
    let result = html_to_carve(STEPS, &on()).unwrap();
    assert_eq!(result.value, STEPS_CARVE);
    assert_eq!(result.report.diagnostics, Vec::new());
}

#[test]
fn the_option_is_off_by_default() {
    let result = html_to_carve(STEPS, &HtmlImportOptions::default()).unwrap();
    assert_eq!(
        result.value,
        "|= Step |= Detail |\n|= 1 | Install. `npm i` |\n| \\^ | < |\n^ Steps\n"
    );
}

#[test]
fn a_table_whose_cells_are_all_inline_keeps_the_pipe_form() {
    let html = "<table><tr><th>A</th></tr><tr><td><p>x</p></td></tr></table>";
    assert_eq!(
        html_to_carve(html, &on()).unwrap().value,
        html_to_carve(html, &HtmlImportOptions::default())
            .unwrap()
            .value
    );
}

#[test]
fn header_columns_are_counted_over_the_grid_and_a_blank_row_stays() {
    let html = "<table><tr><th>H</th><th>I</th></tr>\
<tr><th rowspan=\"2\">R</th><td><ul><li>a</li></ul></td></tr>\
<tr><td>b</td></tr><tr><th></th><td></td></tr></table>";
    assert_eq!(
        html_to_carve(html, &on()).unwrap().value,
        "{header-rows=1 header-cols=1}\n::: list-table\n- - H\n  - I\n- - R\n  - - a\n\
- - ^\n  - b\n- - +\n  - +\n:::\n"
    );
}

#[test]
fn a_cell_keeps_its_attributes_and_a_row_reports_dropping_its_own() {
    let html = "<table id=\"t\"><tr id=\"r\"><td class=\"k\"><p>a</p><p>b</p></td></tr></table>";
    let result = html_to_carve(html, &on()).unwrap();
    assert_eq!(
        result.value,
        "{#t}\n::: list-table\n- -{.k} a\n\n    b\n:::\n"
    );
    let rows: Vec<_> = result
        .report
        .diagnostics
        .iter()
        .map(|d| (d.code, d.severity, d.path.as_deref()))
        .collect();
    assert_eq!(
        rows,
        vec![(
            HtmlImportDiagnosticCode::AttributeDropped,
            HtmlImportSeverity::Info,
            Some("/table[1]/tr[1]")
        )]
    );
}

#[test]
fn a_row_with_no_cells_is_dropped_and_reported() {
    let html = "<table><tr></tr><tr><th>H</th></tr><tr><td><p>a</p><p>b</p></td></tr></table>";
    let result = html_to_carve(html, &on()).unwrap();
    assert_eq!(
        result.value,
        "{header-rows=1}\n::: list-table\n- - H\n- - a\n\n    b\n:::\n"
    );
    let rows: Vec<_> = result
        .report
        .diagnostics
        .iter()
        .map(|d| (d.code, d.path.as_deref()))
        .collect();
    assert_eq!(
        rows,
        vec![(
            HtmlImportDiagnosticCode::StructureUnspellable,
            Some("/table[1]/tr[1]")
        )]
    );
}

#[test]
fn the_ast_exit_reports_the_lost_row_grouping_too() {
    let html = "<table><tbody id=\"b\"><tr><td><p>a</p><p>b</p></td></tr></tbody></table>";
    let codes = |d: &[carve::HtmlImportDiagnostic]| d.iter().map(|d| d.code).collect::<Vec<_>>();
    let ast = html_to_ast(html, &on()).unwrap().report.diagnostics;
    let source = html_to_carve(html, &on()).unwrap().report.diagnostics;
    assert_eq!(codes(&ast), codes(&source));
    assert!(codes(&ast).contains(&HtmlImportDiagnosticCode::StructureUnspellable));
}

#[test]
fn the_ast_exit_publishes_the_admonition() {
    let doc = html_to_ast(STEPS, &on()).unwrap().value;
    let json: serde_json::Value = serde_json::from_str(&carve::to_json(&doc)).unwrap();
    assert_eq!(json["children"][0]["type"], "admonition");
    assert_eq!(json["children"][0]["kind"], "list-table");
}

#[test]
fn the_cli_spells_it_list_table() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_carve"))
        .args(["migrate", "--from", "html", "--list-table"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(STEPS.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), STEPS_CARVE);
}

#[test]
fn the_markdown_target_writes_a_list_table_as_a_pipe_table() {
    assert_eq!(
        to_markdown(STEPS_CARVE),
        "| Step | Detail |\n| --- | --- |\n| 1 | Install. npm i |\n| ^ |  |\n\nSteps\n"
    );
    assert_eq!(
        to_markdown(
            "{aligns=\"right\" header-cols=1}\n::: list-table\n- - a\n  -{header} b\n:::\n"
        ),
        "| a | b |\n| ---: | --- |\n"
    );
}

#[test]
fn every_list_in_a_row_gives_cells_and_the_label_is_kept() {
    assert_eq!(
        to_markdown("::: list-table [Lbl]\n- - a\n  - b\n\n  1. c\n  2. d\n:::\n"),
        "**Lbl**\n\n|  |  |  |  |\n| --- | --- | --- | --- |\n| a | b | c | d |\n"
    );
}

#[test]
fn a_body_that_is_not_a_grid_keeps_the_old_writing() {
    assert_eq!(
        to_markdown("::: list-table \"T\"\n- one\n- two\n:::\n"),
        "**T**\n\n- one\n- two\n"
    );
}
