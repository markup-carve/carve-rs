//! `markup-carve/carve#1210` P1, carve-rs's row-grouping row: a table's
//! `<thead>` / `<tbody>` / `<tfoot>` reach `table.rowGroups` (PART 12 §15), but
//! only where the partition says something a reader cannot derive from the rows
//! (D1, ruled as (b)).
//!
//! The field had a wire allowlist entry and no type, no producer and no
//! consumer here. Carve 0.1 source has no spelling for it, so `html_to_ast`
//! states the partition and `html_to_carve` reports the loss, which is the split
//! PART 12 §16 draws.

use carve::{
    from_json, html_to_ast, html_to_carve, to_json, HtmlImportDiagnosticCode, HtmlImportOptions,
    HtmlImportSeverity, TableBodyGroup, TableRowGroups,
};

fn groups_of(html: &str) -> Option<TableRowGroups> {
    let doc = html_to_ast(html, &HtmlImportOptions::default())
        .unwrap()
        .value;
    match doc.children.first() {
        Some(carve::BlockNode::Table(table)) => table.row_groups.clone(),
        other => panic!("expected a table, got {other:?}"),
    }
}

fn body(head_rows: usize, body_rows: usize, row_head_columns: Option<usize>) -> TableBodyGroup {
    TableBodyGroup {
        head_rows,
        body_rows,
        row_head_columns,
        attrs: None,
    }
}

/// Every renderer derives the leading run of all-header rows as the head, the
/// rest as one body, no foot and no row-head columns. A table that says exactly
/// that says nothing extra, or the field would land on nearly every document.
#[test]
fn a_table_every_renderer_already_derives_states_nothing() {
    for html in [
        "<table><thead><tr><th>a</th></tr></thead><tbody><tr><td>1</td></tr></tbody></table>",
        "<table><tr><th>a</th></tr><tr><td>1</td></tr></table>",
        "<table><tbody><tr><th>a</th></tr><tr><td>1</td></tr></tbody></table>",
        "<table><tr><td>1</td></tr><tr><td>2</td></tr></table>",
    ] {
        assert_eq!(groups_of(html), None, "{html}");
    }
}

/// The five shapes where the stated partition and the derived one DISAGREE.
#[test]
fn a_partition_a_reader_cannot_derive_is_stated() {
    for (name, html, expected) in [
        (
            "a foot",
            "<table><thead><tr><th>a</th></tr></thead><tbody><tr><td>1</td></tr></tbody><tfoot><tr><td>f</td></tr></tfoot></table>",
            TableRowGroups { head_rows: 1, bodies: vec![body(0, 1, None)], foot_rows: 1 },
        ),
        (
            "a second body",
            "<table><tbody><tr><td>1</td></tr></tbody><tbody><tr><td>2</td></tr></tbody></table>",
            TableRowGroups { head_rows: 0, bodies: vec![body(0, 1, None), body(0, 1, None)], foot_rows: 0 },
        ),
        (
            "row-head columns",
            "<table><thead><tr><th>a</th><th>b</th></tr></thead><tbody><tr><th>R1</th><td>1</td></tr><tr><th>R2</th><td>2</td></tr></tbody></table>",
            TableRowGroups { head_rows: 1, bodies: vec![body(0, 2, Some(1))], foot_rows: 0 },
        ),
        (
            // Word and pandoc both emit this: the derived head is EMPTY and the
            // stated one is not.
            "a head that is not header cells",
            "<table><thead><tr><td>a</td></tr></thead><tbody><tr><td>1</td></tr></tbody></table>",
            TableRowGroups { head_rows: 1, bodies: vec![body(0, 1, None)], foot_rows: 0 },
        ),
        (
            "a body with its own header rows under a head",
            "<table><thead><tr><th>a</th></tr></thead><tbody><tr><th>m</th></tr><tr><td>1</td></tr></tbody></table>",
            TableRowGroups { head_rows: 1, bodies: vec![body(1, 1, None)], foot_rows: 0 },
        ),
    ] {
        assert_eq!(groups_of(html), Some(expected), "{name}");
    }
}

/// A header-only first body stays a body when a second one follows it: that
/// boundary is what the field exists to record, and absorbing it into the head
/// would leave one ordinary body the derivation reproduces, so the two bodies
/// would go in silence.
#[test]
fn a_header_only_first_body_stays_a_body_when_a_second_follows() {
    assert_eq!(
        groups_of(
            "<table><tbody><tr><th>a</th></tr></tbody><tbody><tr><td>1</td></tr></tbody></table>"
        ),
        Some(TableRowGroups {
            head_rows: 0,
            bodies: vec![body(1, 0, None), body(0, 1, None)],
            foot_rows: 0,
        })
    );
}

/// Row-head COLUMNS, which spans make different from cells: a `<th colspan=2>`
/// is one element and two columns, and a `<th rowspan=2>` leaves the row below
/// it starting with a data element while a header still occupies the column.
#[test]
fn row_head_columns_counts_columns_and_not_cells() {
    for (name, html, expected) in [
        (
            "a colspan header",
            "<table><thead><tr><th>a</th><th>b</th><th>c</th></tr></thead><tbody><tr><th colspan=\"2\">R</th><td>1</td></tr><tr><th colspan=\"2\">S</th><td>2</td></tr></tbody></table>",
            2,
        ),
        (
            "a rowspan header",
            "<table><thead><tr><th>a</th><th>b</th></tr></thead><tbody><tr><th rowspan=\"2\">R</th><td>1</td></tr><tr><td>2</td></tr></tbody></table>",
            1,
        ),
        (
            // One slot standing for two columns.
            "a header spanning both ways",
            "<table><thead><tr><th>a</th><th>b</th><th>c</th></tr></thead><tbody><tr><th rowspan=\"2\" colspan=\"2\">R</th><td>1</td></tr><tr><td>2</td></tr></tbody></table>",
            2,
        ),
    ] {
        let groups = groups_of(html).unwrap_or_else(|| panic!("{name}: no partition"));
        assert_eq!(
            groups.bodies[0].row_head_columns,
            Some(expected),
            "{name}: {groups:?}"
        );
    }
}

/// An all-header row inside a body is an intermediate HEADER row, not a row
/// whose every column is a row head. Counting its columns would make the group's
/// minimum that row's width and put a `rowHeadColumns` on a table that has none.
///
/// It has to sit BELOW a data row to be reachable at all - a leading run of them
/// is the group's own header and never reaches the count.
#[test]
fn an_all_header_row_inside_a_body_is_not_a_row_head_column() {
    let groups = groups_of(
        "<table><tbody><tr><th>a</th><th>b</th><td>1</td></tr><tr><th>x</th></tr></tbody><tbody><tr><td>2</td></tr></tbody></table>",
    )
    .expect("two bodies state a partition");
    assert_eq!(groups.bodies[0].row_head_columns, None, "{groups:?}");
}

/// A body does not inherit a row head from the body ABOVE it.
///
/// `row_head_columns` resolves a `^` by walking up, and nothing in that walk
/// stops at a group boundary - it does not need to, because a rowspan is clipped
/// to its own row group when the grid is built (markup-carve/carve-rs#1000), so
/// no `^` ever lands in a later group. That is a coupling between two features
/// rather than a property of either, so it is pinned here: if the clip regresses
/// this states a row head the second body does not have.
#[test]
fn a_body_does_not_inherit_a_row_head_from_the_body_above_it() {
    let groups = groups_of(
        "<table><tbody><tr><td>a</td></tr><tr><th rowspan=\"2\">R</th><td>1</td></tr></tbody><tbody><tr><td>2</td></tr></tbody></table>",
    )
    .expect("two bodies state a partition");
    assert_eq!(groups.bodies.len(), 2, "{groups:?}");
    assert_eq!(groups.bodies[1].row_head_columns, None, "{groups:?}");
}

/// The head is a PREFIX of the rows and the foot a SUFFIX, which is what the
/// field can express. A table this cannot describe is refused and reported
/// rather than described wrongly.
#[test]
fn a_head_or_foot_away_from_the_edge_is_refused_and_reported() {
    let html =
        "<table><tbody><tr><td>1</td></tr></tbody><thead><tr><th>a</th></tr></thead></table>";
    assert_eq!(groups_of(html), None);
    let report = html_to_ast(html, &HtmlImportOptions::default())
        .unwrap()
        .report;
    assert_eq!(report.diagnostics.len(), 1, "{:?}", report.diagnostics);
    assert_eq!(
        report.diagnostics[0].code,
        HtmlImportDiagnosticCode::TableDegraded
    );
    assert_eq!(report.diagnostics[0].severity, HtmlImportSeverity::Warning);
    assert!(
        report.diagnostics[0]
            .message
            .contains("not at the edge of its rows"),
        "{:?}",
        report.diagnostics[0]
    );
}

/// The AST keeps it and says nothing; a WRITER loses it and says so. That split
/// is PART 12 §16, and `structure-unspellable` is the code the import schema
/// names for it - carve-rs produced none of the eight before this.
#[test]
fn the_ast_keeps_the_partition_and_a_writer_reports_losing_it() {
    let html =
        "<table><tbody><tr><td>1</td></tr></tbody><tbody><tr><td>2</td></tr></tbody></table>";
    let ast = html_to_ast(html, &HtmlImportOptions::default()).unwrap();
    assert!(
        ast.report.diagnostics.is_empty(),
        "the AST path lost nothing: {:?}",
        ast.report.diagnostics
    );

    let written = html_to_carve(html, &HtmlImportOptions::default()).unwrap();
    assert_eq!(written.value, "| 1 |\n| 2 |\n");
    assert_eq!(
        written.report.diagnostics.len(),
        1,
        "{:?}",
        written.report.diagnostics
    );
    assert_eq!(
        written.report.diagnostics[0].code,
        HtmlImportDiagnosticCode::StructureUnspellable
    );
    assert_eq!(
        written.report.diagnostics[0].severity,
        HtmlImportSeverity::Warning
    );
    assert!(
        written.report.diagnostics[0]
            .message
            .contains("explicit head/body/foot grouping"),
        "{:?}",
        written.report.diagnostics[0]
    );
}

/// CONTROL: a table whose partition IS derivable reports nothing on either
/// path. A loss report that fires on every table is one nobody reads.
#[test]
fn a_derivable_table_reports_no_writer_loss() {
    let written = html_to_carve(
        "<table><thead><tr><th>a</th></tr></thead><tbody><tr><td>1</td></tr></tbody></table>",
        &HtmlImportOptions::default(),
    )
    .unwrap();
    assert_eq!(written.report.diagnostics, vec![]);
}

/// The field survives the wire in both directions.
#[test]
fn the_partition_round_trips_through_ast_json() {
    let html = "<table><thead><tr><th>a</th></tr></thead><tbody><tr><td>1</td></tr></tbody><tfoot><tr><td>f</td></tr></tfoot></table>";
    let doc = html_to_ast(html, &HtmlImportOptions::default())
        .unwrap()
        .value;
    let json = to_json(&doc);
    assert!(
        json.contains("\"rowGroups\":{\"headRows\":1,\"bodies\":[{\"headRows\":0,\"bodyRows\":1}],\"footRows\":1}"),
        "{json}"
    );
    let back = from_json(&json).unwrap();
    assert_eq!(to_json(&back), json);
}

/// PART 12 §15's summing MUST, checked on the INPUT path because that is where
/// it CAN fail. JSON Schema cannot express a sum across fields, so a
/// non-summing partition validates against the schema; and the importer builds
/// the counts from the same row list the rows come from, so a check there could
/// not fail.
#[test]
fn a_partition_that_does_not_consume_the_rows_is_refused() {
    let two_rows = |groups: &str| {
        format!(
            "{{\"type\":\"document\",\"children\":[{{\"type\":\"table\",\"rows\":[{{\"type\":\"table_row\",\"cells\":[]}},{{\"type\":\"table_row\",\"cells\":[]}}],\"rowGroups\":{groups}}}],\"srcByteLength\":0}}"
        )
    };
    for groups in [
        "{\"headRows\":5,\"bodies\":[],\"footRows\":0}",
        "{\"headRows\":0,\"bodies\":[],\"footRows\":0}",
        "{\"headRows\":1,\"bodies\":[{\"headRows\":1,\"bodyRows\":1}],\"footRows\":0}",
    ] {
        let error = from_json(&two_rows(groups)).expect_err(groups);
        assert!(
            error.to_string().contains("does not partition"),
            "{groups}: {error}"
        );
    }
    // Both numbers are named, so the payload can be fixed.
    let error = from_json(&two_rows(
        "{\"headRows\":3,\"bodies\":[{\"headRows\":1,\"bodyRows\":1}],\"footRows\":1}",
    ))
    .unwrap_err();
    assert!(error.to_string().contains("6 rows of 2"), "{error}");

    // Every count is a number off untrusted JSON and nothing bounds it, so the
    // sum is CHECKED: two counts near the maximum wrap to a small total in
    // release and panic in debug, which would both abort the process and, having
    // wrapped, accept a partition that consumes nothing.
    let huge = 9_223_372_036_854_775_807u64;
    let error = from_json(&two_rows(&format!(
        "{{\"headRows\":{huge},\"bodies\":[{{\"headRows\":{huge},\"bodyRows\":2}}],\"footRows\":0}}"
    )))
    .unwrap_err();
    assert!(error.to_string().contains("does not partition"), "{error}");

    // CONTROL: a partition that DOES consume them is accepted, and a table
    // without the field is not asked the question.
    for groups in [
        "{\"headRows\":1,\"bodies\":[{\"headRows\":0,\"bodyRows\":1}],\"footRows\":0}",
        "{\"headRows\":0,\"bodies\":[{\"headRows\":1,\"bodyRows\":1}],\"footRows\":0}",
    ] {
        from_json(&two_rows(groups)).unwrap_or_else(|e| panic!("{groups}: {e}"));
    }
}

/// A key the schema does not name inside the partition is refused like any
/// other (PART 12 §11). `rowGroups` is written INLINE on the table rather than
/// pulled from `$defs`, so the generated field map could not see it or its body
/// groups, and both rode straight in.
#[test]
fn a_key_the_schema_does_not_name_inside_the_partition_is_refused() {
    let payload = |groups: &str| {
        format!(
            "{{\"type\":\"document\",\"children\":[{{\"type\":\"table\",\"rows\":[{{\"type\":\"table_row\",\"cells\":[]}}],\"rowGroups\":{groups}}}],\"srcByteLength\":0}}"
        )
    };
    for groups in [
        "{\"headRows\":1,\"bodies\":[],\"footRows\":0,\"bogus\":1}",
        "{\"headRows\":0,\"bodies\":[{\"headRows\":0,\"bodyRows\":1,\"bogus\":1}],\"footRows\":0}",
    ] {
        let error = from_json(&payload(groups)).expect_err(groups);
        assert!(
            error.to_string().contains("the schema does not name"),
            "{groups}: {error}"
        );
    }
}
