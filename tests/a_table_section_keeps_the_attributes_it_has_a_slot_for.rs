//! A `<tbody>`'s and a `<tr>`'s own attributes reach the slots the model has for
//! them, and the sections with no slot are named on the way out.
//!
//! Nothing read any of them before. Measured on this engine before the change:
//! `<table><tbody id="body" class="x"><tr><td>1</td></tr></tbody></table>`
//! imported with `row_groups: None` and not one diagnostic, and
//! `<table><tr id="r1"><td>a</td></tr></table>` wrote `| a |`. Both fell into
//! the empty `attrs` slot in silence - the exact loss `markup-carve/carve#1210`
//! exists to kill - though `TableRow::attrs` is spelled by the writer on the
//! closing pipe and `TableBodyGroup::attrs` is in PART 12's table model.
//!
//! Only a BODY has a section slot. The head and the foot are stated as row
//! COUNTS, so attributes on `<thead>` or `<tfoot>` cannot be represented at all
//! and are reported instead. Ported from `markup-carve/carve-js#1096`.

use carve::{
    from_json, html_to_ast, html_to_carve, to_json, Attrs, BlockNode, HtmlImportDiagnostic,
    HtmlImportDiagnosticCode, HtmlImportOptions, HtmlImportSeverity, TableBodyGroup,
    TableRowGroups,
};

fn table_of(html: &str) -> carve::Table {
    let doc = html_to_ast(html, &HtmlImportOptions::default())
        .unwrap()
        .value;
    match doc.children.first() {
        Some(BlockNode::Table(table)) => table.clone(),
        other => panic!("expected a table, got {other:?}"),
    }
}

fn diagnostics(html: &str) -> Vec<HtmlImportDiagnostic> {
    html_to_carve(html, &HtmlImportOptions::default())
        .unwrap()
        .report
        .diagnostics
}

fn messages(html: &str, code: HtmlImportDiagnosticCode) -> Vec<String> {
    diagnostics(html)
        .into_iter()
        .filter(|d| d.code == code)
        .map(|d| d.message)
        .collect()
}

/// The PATHS the reports point at, which is the half that tells a reader WHICH
/// element lost something rather than that one did. Asserted separately because
/// a message-only assertion cannot tell a threaded path from the table's own.
fn paths(html: &str, code: HtmlImportDiagnosticCode) -> Vec<String> {
    diagnostics(html)
        .into_iter()
        .filter(|d| d.code == code)
        .map(|d| d.path.unwrap_or_default())
        .collect()
}

fn attrs(id: Option<&str>, classes: &[&str], key_values: &[(&str, &str)]) -> Attrs {
    Attrs {
        id: id.map(str::to_owned),
        classes: classes.iter().map(|c| (*c).to_owned()).collect(),
        key_values: key_values
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect(),
        order: Vec::new(),
    }
}

/// The body group's `attrs` is where a `<tbody>`'s own attributes go, and a body
/// carrying any is a partition no reader can DERIVE - the derivation has no way
/// to say them - so the field is emitted to hold them.
#[test]
fn a_body_section_puts_its_attributes_in_the_group() {
    let table = table_of(
        "<table><thead><tr><th>a</th></tr></thead><tbody id=\"body\" class=\"x\"><tr><td>1</td></tr></tbody></table>",
    );
    assert_eq!(
        table.row_groups,
        Some(TableRowGroups {
            head_rows: 1,
            bodies: vec![TableBodyGroup {
                head_rows: 0,
                body_rows: 1,
                row_head_columns: None,
                attrs: Some(attrs(Some("body"), &["x"], &[])),
            }],
            foot_rows: 0,
        })
    );
    // Nothing is REPORTED for the attributes: they were carried, not lost. The
    // one diagnostic is the field's own unspellability in Carve source, which
    // was already the rule.
    assert_eq!(
        messages(
            "<table><thead><tr><th>a</th></tr></thead><tbody id=\"body\" class=\"x\"><tr><td>1</td></tr></tbody></table>",
            HtmlImportDiagnosticCode::AttributeDropped
        ),
        Vec::<String>::new()
    );
}

/// The same table WITHOUT the attributes states nothing, so the field is not
/// being put on every imported table: the attributes are what make it
/// non-derivable here.
#[test]
fn the_same_table_without_them_still_states_nothing() {
    assert_eq!(
        table_of(
            "<table><thead><tr><th>a</th></tr></thead><tbody><tr><td>1</td></tr></tbody></table>"
        )
        .row_groups,
        None
    );
}

/// A `data-` pair reaches the group too, not only an id and a class.
#[test]
fn a_body_group_carries_a_data_attribute() {
    let table = table_of("<table><tbody data-k=\"v\"><tr><td>1</td></tr></tbody></table>");
    assert_eq!(
        table.row_groups.as_ref().map(|g| g.bodies[0].attrs.clone()),
        Some(Some(attrs(None, &[], &[("data-k", "v")])))
    );
}

/// The group survives the JSON wire, which is the only place it can be read
/// back: Carve SOURCE has no spelling for it.
#[test]
fn a_body_group_with_attributes_survives_the_wire() {
    let doc = html_to_ast(
        "<table><tbody id=\"body\"><tr><td>1</td></tr></tbody></table>",
        &HtmlImportOptions::default(),
    )
    .unwrap()
    .value;
    let json = to_json(&doc);
    assert!(
        json.contains("\"attrs\":{\"id\":\"body\"}"),
        "the group's attrs are on the wire: {json}"
    );
    // `children` rather than the whole document: `from_json` records the
    // payload length it read, which the import side has no equivalent of.
    assert_eq!(from_json(&json).unwrap().children, doc.children);
}

/// A `<tr>`'s attributes have a slot of their own, and the writer spells them on
/// the closing pipe, so the import ROUND TRIPS and nothing is reported.
#[test]
fn a_row_keeps_its_own_attributes() {
    let html = "<table><tr id=\"r1\" class=\"hi\"><td>a</td></tr></table>";
    assert_eq!(
        table_of(html).rows[0].attrs,
        Some(attrs(Some("r1"), &["hi"], &[]))
    );
    let written = html_to_carve(html, &HtmlImportOptions::default())
        .unwrap()
        .value;
    assert_eq!(written, "| a |{#r1 .hi}\n");
    assert_eq!(
        carve::to_html(&written),
        "<table>\n  <tbody>\n    <tr id=\"r1\" class=\"hi\"><td>a</td></tr>\n  </tbody>\n</table>"
    );
    assert_eq!(diagnostics(html), Vec::new());
}

/// A row attribute lands on the row it was written on, including the rows a
/// rowspan reaches into - the built grid has more rows than the source has
/// cells, and the attributes follow the SOURCE rows.
#[test]
fn a_row_attribute_follows_its_own_row_through_the_spans() {
    let written = html_to_carve(
        "<table><tr id=\"r1\"><td rowspan=\"2\">a</td><td>b</td></tr><tr id=\"r2\"><td>c</td></tr></table>",
        &HtmlImportOptions::default(),
    )
    .unwrap()
    .value;
    assert_eq!(written, "| a | b |{#r1}\n| ^ | c |{#r2}\n");
}

/// The head and the foot are stated as row COUNTS. There is no slot, so the
/// attributes are named on the way out rather than dropped in silence.
#[test]
fn a_head_or_a_foot_is_reported_by_name() {
    assert_eq!(
        messages(
            "<table><thead id=\"h\"><tr><th>a</th></tr></thead><tbody><tr><td>1</td></tr></tbody></table>",
            HtmlImportDiagnosticCode::AttributeDropped
        ),
        vec![
            "Dropped id on <thead>: a table's head is stated as a row count and has no attribute slot"
        ]
    );
    assert_eq!(
        messages(
            "<table><thead><tr><th>a</th></tr></thead><tbody><tr><td>1</td></tr></tbody><tfoot class=\"f\" data-k=\"v\"><tr><td>x</td></tr></tfoot></table>",
            HtmlImportDiagnosticCode::AttributeDropped
        ),
        vec![
            "Dropped class, data-k on <tfoot>: a table's foot is stated as a row count and has no attribute slot"
        ]
    );
    // The report points at the SECTION, not at the table: the path is threaded
    // through the row walk for exactly this.
    assert_eq!(
        paths(
            "<table><thead id=\"h\"><tr><th>a</th></tr></thead><tbody><tr><td>1</td></tr></tbody><tfoot id=\"f\"><tr><td>x</td></tr></tfoot></table>",
            HtmlImportDiagnosticCode::AttributeDropped
        ),
        vec![
            "/table[1]/thead[1]",
            "/table[1]/tfoot[3]",
        ]
    );
}

/// A body group IS the run of rows it consumes, so a section with none is not a
/// group and has nowhere to put them.
///
/// This is the shape a list read back off the ROWS never sees: the sections are
/// collected on the way through the table for exactly this one.
#[test]
fn a_section_with_no_rows_is_reported() {
    assert_eq!(
        messages(
            "<table><tbody id=\"empty\"></tbody><tbody><tr><td>1</td></tr></tbody></table>",
            HtmlImportDiagnosticCode::AttributeDropped
        ),
        vec!["Dropped id on <tbody>: a body group is the rows it consumes, and this one has none"]
    );
    assert_eq!(
        paths(
            "<table><tbody id=\"empty\"></tbody><tbody><tr><td>1</td></tr></tbody></table>",
            HtmlImportDiagnosticCode::AttributeDropped
        ),
        vec!["/table[1]/tbody[1]"]
    );
}

/// A `<thead>` that is not a prefix of the rows drops the WHOLE grouping, and a
/// `<tbody>`'s attributes reach nothing when the field itself is not kept.
#[test]
fn a_body_whose_grouping_was_dropped_is_reported() {
    let html =
        "<table><tbody id=\"b1\"><tr><td>1</td></tr></tbody><thead><tr><th>a</th></tr></thead></table>";
    assert_eq!(table_of(html).row_groups, None);
    assert_eq!(
        messages(html, HtmlImportDiagnosticCode::AttributeDropped),
        vec![
            "Dropped id on <tbody>: the row grouping this body belongs to was not kept, and nothing else holds it"
        ]
    );
}

/// A group whose counts are both ZERO is not empty when it carries attributes,
/// and dropping it after the head absorbs an all-header body would take them
/// with it.
#[test]
fn a_zero_count_body_group_survives_absorption_when_it_carries_attributes() {
    assert_eq!(
        table_of("<table><tbody id=\"hdr\"><tr><th>a</th></tr></tbody></table>").row_groups,
        Some(TableRowGroups {
            head_rows: 1,
            bodies: vec![TableBodyGroup {
                head_rows: 0,
                body_rows: 0,
                row_head_columns: None,
                attrs: Some(attrs(Some("hdr"), &[], &[])),
            }],
            foot_rows: 0,
        })
    );
    // And with NOTHING to carry, the same absorption still empties the group
    // away, or the field would land on the ordinary header-and-rows table.
    assert_eq!(
        table_of("<table><tbody><tr><th>a</th></tr></tbody></table>").row_groups,
        None
    );
}

/// Reading these elements at all puts them on the ordinary attribute path, so an
/// unsupported attribute on a section or a row reports the way it does anywhere
/// else. Nothing was said about them before.
#[test]
fn an_unsupported_attribute_on_a_row_or_a_section_reports_as_it_does_elsewhere() {
    assert_eq!(
        messages(
            "<table><tr onclick=\"x\"><td>a</td></tr></table>",
            HtmlImportDiagnosticCode::AttributeDropped
        ),
        vec!["Dropped event-handler attribute onclick on <tr>"]
    );
    // And at the ROW it was on, not at the table.
    assert_eq!(
        paths(
            "<table><tr><td>a</td></tr><tr onclick=\"x\"><td>b</td></tr></table>",
            HtmlImportDiagnosticCode::AttributeDropped
        ),
        vec!["/table[1]/tr[2]"]
    );
    // A body's OWN attribute is no longer dropped on the way in: the importer
    // keeps every attribute Carve can hold, and a body group is the one section
    // with a slot for them (carve-rs#1060). It reaches the AST, so nothing is
    // reported dropped; what the Carve WRITER cannot spell is the grouping
    // itself, which is the separate report this table already carries.
    let html = "<table><tbody align=\"left\"><tr><td>a</td></tr></tbody></table>";
    assert_eq!(
        messages(html, HtmlImportDiagnosticCode::AttributeDropped),
        Vec::<String>::new()
    );
    assert_eq!(
        table_of(html)
            .row_groups
            .as_ref()
            .and_then(|g| g.bodies[0].attrs.clone())
            .map(|a| a.key_values),
        Some(
            [("align".to_string(), "left".to_string())]
                .into_iter()
                .collect()
        )
    );
}

/// A `<colgroup>` and its `<col>` children state COLUMN structure, and Carve has
/// no column model at all to put it in. It was dropped in silence; whether the
/// language wants a column model is a separate question, but a loss the reader
/// is never told about is the one they cannot work around.
#[test]
fn a_column_group_is_reported_rather_than_dropped_in_silence() {
    let html =
        "<table><colgroup><col span=\"2\" class=\"c\"></colgroup><tr><td>a</td><td>b</td></tr></table>";
    assert_eq!(
        messages(html, HtmlImportDiagnosticCode::ElementDropped),
        vec![
            "Dropped <colgroup>: Carve has no column model, and a table's columns are only the cells its rows carry"
        ]
    );
    assert_eq!(
        diagnostics(html)
            .into_iter()
            .find(|d| d.code == HtmlImportDiagnosticCode::ElementDropped)
            .map(|d| (d.severity, d.path)),
        Some((
            HtmlImportSeverity::Warning,
            Some("/table[1]/colgroup[1]".to_owned())
        ))
    );
    // Two of them are two reports, and the report covers the `<col>`s inside the
    // wrapper the way one report covers a dropped subtree everywhere else.
    assert_eq!(
        paths(
            "<table><colgroup><col></colgroup><colgroup><col><col></colgroup><tr><td>a</td></tr></table>",
            HtmlImportDiagnosticCode::ElementDropped
        ),
        vec![
            "/table[1]/colgroup[1]",
            "/table[1]/colgroup[2]",
        ]
    );
    // A `<col>` written with NO wrapper is reported through the one the parser
    // inserts for it, and a run of them arrives as a single wrapper - which is
    // why only `<colgroup>` is looked for. A `<col>` is never a direct child of
    // a `<table>` after parsing, so an arm matching one could not fire.
    assert_eq!(
        paths(
            "<table><col span=\"2\"><col><tr><td>a</td><td>b</td></tr></table>",
            HtmlImportDiagnosticCode::ElementDropped
        ),
        vec!["/table[1]/colgroup[1]"]
    );
    // One that opens BELOW the rows is its own wrapper, at its own path.
    assert_eq!(
        paths(
            "<table><tr><td>a</td></tr><col></table>",
            HtmlImportDiagnosticCode::ElementDropped
        ),
        vec!["/table[1]/colgroup[2]"]
    );
    // A table with no column structure says nothing.
    assert_eq!(
        messages(
            "<table><tr><td>a</td></tr></table>",
            HtmlImportDiagnosticCode::ElementDropped
        ),
        Vec::<String>::new()
    );
}

/// CONTROLS: span shapes this change did not touch, pinned so a regression in
/// the grid shows up here rather than in the attribute assertions above.
///
/// A cell spanning BOTH ways is the one `markup-carve/carve-js#1096`'s sibling
/// found broken there and correct here: `span_grid` carries a mark into every
/// column a spanning cell covers, so a `colspan="2" rowspan="2"` writes two `^`
/// on the row below and re-reads as the grid it came with.
#[test]
fn the_span_grid_is_unchanged() {
    for (html, written) in [
        (
            "<table><tr><td colspan=\"2\" rowspan=\"2\">a</td><td>b</td></tr><tr><td>c</td></tr><tr><td>d</td><td>e</td><td>f</td></tr></table>",
            "| a | < | b |\n| ^ | ^ | c |\n| d | e | f |\n",
        ),
        (
            "<table><tr><td colspan=\"2\">a</td></tr><tr><td>b</td><td>c</td></tr></table>",
            "| a | < |\n| b | c |\n",
        ),
        (
            "<table><tr><td rowspan=\"2\">a</td><td>b</td></tr><tr><td>c</td></tr></table>",
            "| a | b |\n| ^ | c |\n",
        ),
        (
            // `rowspan="0"` is "to the end of the row group", resolved against
            // the group the row is in.
            "<table><tbody><tr><td rowspan=\"0\">a</td><td>b</td></tr><tr><td>c</td></tr></tbody></table>",
            "| a | b |\n| ^ | c |\n",
        ),
        (
            // A rowspan stops at its row GROUP, so a `<tfoot>` below is not
            // swallowed.
            "<table><tbody><tr><td rowspan=\"3\">a</td><td>b</td></tr></tbody><tfoot><tr><td>f</td></tr></tfoot></table>",
            "| a | b |\n| f |\n",
        ),
        (
            "<table><tr><td colspan=\"5\">a</td></tr><tr><td>b</td></tr></table>",
            "| a | < | < | < | < |\n| b |\n",
        ),
        (
            "<table><tr><td colspan=\"2\" class=\"x\">a</td></tr><tr><td>b</td><td>c</td></tr></table>",
            "|{.x} a | < |\n| b | c |\n",
        ),
    ] {
        assert_eq!(
            html_to_carve(html, &HtmlImportOptions::default())
                .unwrap()
                .value,
            written,
            "{html}"
        );
    }
}

/// CONTROL: a rowspan leaving the leading header run is clipped and reported,
/// which is a `table-degraded` and not one of the codes this change emits.
#[test]
fn a_rowspan_leaving_the_header_run_is_still_clipped() {
    assert_eq!(
        messages(
            "<table><tr><th rowspan=\"2\">a</th><th>b</th></tr><tr><td>c</td></tr></table>",
            HtmlImportDiagnosticCode::TableDegraded
        ),
        vec![
            "Clipped a rowspan at the header rows: Carve derives the head from the leading header rows, and a span leaving them crosses a boundary browsers clip anyway"
        ]
    );
}

/// CONTROL: the five partitions a reader cannot derive still state themselves
/// with no attributes anywhere in sight.
#[test]
fn a_partition_a_reader_cannot_derive_is_still_stated() {
    assert_eq!(
        table_of("<table><thead><tr><th>a</th></tr></thead><tbody><tr><td>1</td></tr></tbody><tfoot><tr><td>f</td></tr></tfoot></table>")
            .row_groups,
        Some(TableRowGroups {
            head_rows: 1,
            bodies: vec![TableBodyGroup {
                head_rows: 0,
                body_rows: 1,
                row_head_columns: None,
                attrs: None,
            }],
            foot_rows: 1,
        })
    );
    assert_eq!(
        table_of(
            "<table><tbody><tr><td>1</td></tr></tbody><tbody><tr><td>2</td></tr></tbody></table>"
        )
        .row_groups,
        Some(TableRowGroups {
            head_rows: 0,
            bodies: vec![
                TableBodyGroup {
                    head_rows: 0,
                    body_rows: 1,
                    row_head_columns: None,
                    attrs: None,
                },
                TableBodyGroup {
                    head_rows: 0,
                    body_rows: 1,
                    row_head_columns: None,
                    attrs: None,
                },
            ],
            foot_rows: 0,
        })
    );
}
