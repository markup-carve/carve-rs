//! `markup-carve/carve#1210` P1, carve-rs's span row: an HTML table's
//! `colspan` and `rowspan` become the continuation cells Carve already has for
//! them (`<` continues the cell to the left, `^` the cell above) instead of
//! being flattened away with a diagnostic.
//!
//! The claim these tests make is about the GRID on the other side, so each one
//! reads the imported source back through the HTML renderer and asserts the
//! table a browser would lay out - not the bytes the writer chose.

use carve::{
    html_to_ast, html_to_carve, to_html, HtmlImportDiagnosticCode, HtmlImportError,
    HtmlImportOptions, HtmlImportSeverity,
};

/// The imported source, and the table it renders back to.
fn round_trip(html: &str) -> (String, String) {
    let result = html_to_carve(html, &HtmlImportOptions::default()).unwrap();
    let rendered = to_html(&result.value);
    (result.value, rendered)
}

fn diagnostics(html: &str) -> Vec<(HtmlImportDiagnosticCode, HtmlImportSeverity, String)> {
    let result = html_to_carve(html, &HtmlImportOptions::default()).unwrap();
    result
        .report
        .diagnostics
        .into_iter()
        .map(|d| (d.code, d.severity, d.message))
        .collect()
}

/// Every shape below round-trips its grid and reports nothing: the spans are
/// represented, not degraded.
#[test]
fn a_span_survives_the_import_as_a_continuation_cell() {
    for (name, html, source, table) in [
        (
            "a header-wide column",
            "<table><tr><th>a</th><th>b</th></tr><tr><td colspan=\"2\">wide</td></tr></table>",
            "|= a |= b |\n| wide | < |\n",
            "<tr><td colspan=\"2\">wide</td></tr>",
        ),
        (
            "a cell held over a row",
            "<table><tr><th>a</th><th>b</th></tr><tr><td rowspan=\"2\">tall</td><td>x</td></tr><tr><td>y</td></tr></table>",
            "|= a |= b |\n| tall | x |\n| ^ | y |\n",
            "<tr><td rowspan=\"2\">tall</td><td>x</td></tr>",
        ),
        (
            "a two-by-two merge",
            "<table><tr><th>a</th><th>b</th><th>c</th></tr><tr><td colspan=\"2\" rowspan=\"2\">X</td><td>c</td></tr><tr><td>f</td></tr></table>",
            // A cell spanning BOTH ways carries a mark into each column it
            // covers, so the row below it opens with two `^` rather than one.
            "|= a |= b |= c |\n| X | < | c |\n| ^ | ^ | f |\n",
            "<tr><td rowspan=\"2\" colspan=\"2\">X</td><td>c</td></tr>",
        ),
        (
            "a two-way span beside another rowspan",
            "<table><tr><td colspan=\"2\" rowspan=\"2\">A</td><td rowspan=\"2\">B</td></tr><tr></tr><tr><td>1</td><td>2</td><td>3</td></tr></table>",
            "| A | < | B |\n| ^ | ^ | ^ |\n| 1 | 2 | 3 |\n",
            "<tr><td rowspan=\"2\" colspan=\"2\">A</td><td rowspan=\"2\">B</td></tr>",
        ),
        (
            "a span in the last column",
            "<table><tr><td>a</td><td rowspan=\"2\">b</td></tr><tr><td>c</td></tr></table>",
            "| a | b |\n| c | ^ |\n",
            "<tr><td>a</td><td rowspan=\"2\">b</td></tr>",
        ),
        (
            "two spans in one row",
            "<table><tr><td colspan=\"2\">a</td><td colspan=\"2\">b</td></tr><tr><td>1</td><td>2</td><td>3</td><td>4</td></tr></table>",
            "| a | < | b | < |\n| 1 | 2 | 3 | 4 |\n",
            "<tr><td colspan=\"2\">a</td><td colspan=\"2\">b</td></tr>",
        ),
        (
            "a header cell spanning columns",
            "<table><tr><th colspan=\"2\">Group</th></tr><tr><th>a</th><th>b</th></tr><tr><td>1</td><td>2</td></tr></table>",
            // `span_cell` is an ALTERNATIVE to `header_cell` in the grammar, so
            // a header row carrying one is promoted by the delimiter row
            // instead of by an `=` on each cell.
            "| Group | < |\n|---|---|\n|= a |= b |\n| 1 | 2 |\n",
            "<tr><th scope=\"col\" colspan=\"2\">Group</th></tr>",
        ),
        (
            "a span three rows deep",
            "<table><tr><td rowspan=\"3\">deep</td><td>1</td></tr><tr><td>2</td></tr><tr><td>3</td></tr></table>",
            "| deep | 1 |\n| ^ | 2 |\n| ^ | 3 |\n",
            "<tr><td rowspan=\"3\">deep</td><td>1</td></tr>",
        ),
        (
            "a Word-shaped merge",
            "<table><tbody><tr><td rowspan=\"2\">Name</td><td colspan=\"2\">Contact</td></tr><tr><td>Phone</td><td>Email</td></tr><tr><td>Ada</td><td>1</td><td>a@b</td></tr></tbody></table>",
            "| Name | Contact | < |\n| ^ | Phone | Email |\n| Ada | 1 | a@b |\n",
            "<tr><td rowspan=\"2\">Name</td><td colspan=\"2\">Contact</td></tr>",
        ),
    ] {
        let (written, rendered) = round_trip(html);
        assert_eq!(written, source, "{name}: the written source");
        assert!(
            rendered.contains(table),
            "{name}: the grid did not survive.\nwritten: {written}\nrendered: {rendered}"
        );
        assert_eq!(diagnostics(html), vec![], "{name}: reported a loss");
    }
}

/// A row every span above it covers has no cells of its own, and gains none.
///
/// The renderer resolves a `^` against the cell at the SAME INDEX above it, so a
/// cell spanning both ways has to carry a mark into each column it covers: with
/// one mark for its origin, the next rowspan in the row resolved against a
/// column it does not own, the gap between them was filled with a cell the
/// source did not have, and the row rendered a `<td>` the table does not have.
#[test]
fn a_row_covered_by_the_spans_above_it_renders_empty() {
    let html = "<table><tr><td colspan=\"2\" rowspan=\"2\">A</td><td rowspan=\"2\">B</td></tr><tr></tr><tr><td>1</td><td>2</td><td>3</td></tr></table>";
    let (_, rendered) = round_trip(html);
    assert!(rendered.contains("<tr></tr>"), "{rendered}");
    assert_eq!(diagnostics(html), vec![]);
}

/// HTML's `rowspan="0"` means "to the end of this row group", so it resolves
/// against the group the row is actually in.
#[test]
fn a_rowspan_of_zero_reaches_the_end_of_its_row_group() {
    let (written, _) = round_trip(
        "<table><thead><tr><th>h</th></tr></thead><tbody><tr><td rowspan=\"0\">b</td><td>x</td></tr><tr><td>y</td></tr></tbody><tfoot><tr><td>f</td></tr><tr><td>g</td></tr></tfoot></table>",
    );
    assert_eq!(written, "|= h |\n| b | x |\n| ^ | y |\n| f |\n| g |\n");
}

/// And a POSITIVE rowspan stops there too, whatever the number says: a browser
/// clips it at the group, so a `<tfoot>` below the body is not swallowed by a
/// cell whose layout stops at the body's last row.
#[test]
fn a_rowspan_stops_at_its_row_group_whatever_the_number_says() {
    let (written, _) = round_trip(
        "<table><tbody><tr><td rowspan=\"5\">b</td><td>x</td></tr></tbody><tfoot><tr><td>f</td></tr><tr><td>g</td></tr></tfoot></table>",
    );
    assert_eq!(written, "| b | x |\n| f |\n| g |\n");
}

/// Carve derives the head from the leading run of all-header rows, so a span
/// leaving that run would land in a `<thead>` with its other rows in the
/// `<tbody>` - which browsers clip anyway. Clipped here instead, where it can
/// be reported.
#[test]
fn a_rowspan_leaving_the_derived_head_is_clipped_and_reported() {
    let html = "<table><tr><th rowspan=\"2\">H</th><th>A</th></tr><tr><td>B</td></tr></table>";
    let (written, rendered) = round_trip(html);
    assert_eq!(written, "|= H |= A |\n| B |\n");
    assert!(
        !rendered.contains("rowspan"),
        "the clipped span was written anyway: {rendered}"
    );
    let reported = diagnostics(html);
    assert_eq!(reported.len(), 1, "{reported:?}");
    assert_eq!(reported[0].0, HtmlImportDiagnosticCode::TableDegraded);
    assert_eq!(reported[0].1, HtmlImportSeverity::Warning);
    assert!(
        reported[0]
            .2
            .starts_with("Clipped a rowspan at the header rows"),
        "{reported:?}"
    );
    let result = html_to_carve(html, &HtmlImportOptions::default()).unwrap();
    assert_eq!(
        result.report.diagnostics[0].path.as_deref(),
        Some("/table[1]/tr[1]/th[1]")
    );
}

/// CONTROL: a span WITHIN the header rows crosses no boundary, so it is kept.
#[test]
fn a_rowspan_inside_the_header_rows_is_untouched() {
    let html = "<table><tr><th rowspan=\"2\">H</th><th>A</th></tr><tr><th>B</th></tr><tr><td>1</td><td>2</td></tr></table>";
    let (written, rendered) = round_trip(html);
    assert_eq!(written, "|= H |= A |\n| ^ |= B |\n| 1 | 2 |\n");
    assert!(
        rendered.contains("rowspan=\"2\""),
        "the span was lost: {rendered}"
    );
    assert_eq!(diagnostics(html), vec![]);
}

/// A row shorter than the spans reaching into it needs an index kept, and a
/// cell invented where nothing owns it. Only the invention is reported.
#[test]
fn the_cell_a_short_row_invents_is_the_only_one_reported() {
    let reported = diagnostics(
        "<table><tr><td>a</td><td>b</td><td rowspan=\"2\">c</td></tr><tr></tr></table>",
    );
    assert_eq!(reported.len(), 1, "{reported:?}");
    assert_eq!(reported[0].0, HtmlImportDiagnosticCode::TableDegraded);
    assert!(
        reported[0]
            .2
            .contains("with a cell the source did not have"),
        "{reported:?}"
    );
}

/// CONTROL: a mark that lands ON the row's own cells invents nothing, so a row
/// merely shorter than the one above is not a degradation.
#[test]
fn a_row_that_needs_no_invented_cell_reports_nothing() {
    let html = "<table><tr><td>a</td><td rowspan=\"2\">b</td></tr><tr><td>c</td></tr></table>";
    assert_eq!(diagnostics(html), vec![]);
    let (written, _) = round_trip(html);
    assert_eq!(written, "| a | b |\n| c | ^ |\n");
}

/// Each unit of a span becomes a CELL, so an unclamped `colspan` is a 30-byte
/// input asking for a billion of them. Both attributes clamp to HTML's own
/// maxima, and every generated cell is charged to `max_nodes` on top of that.
#[test]
fn a_span_cannot_ask_for_more_cells_than_html_allows() {
    let (written, _) = round_trip("<table><tr><td colspan=\"999999999\">x</td></tr></table>");
    // 1000 columns, so 1001 pipes.
    assert_eq!(written.matches('|').count(), 1001, "{written}");

    // A value past what a number carries exactly defaults rather than clamping,
    // which is where carve-js lands too.
    let (huge, _) =
        round_trip("<table><tr><td colspan=\"99999999999999999999\">x</td></tr></table>");
    assert_eq!(huge, "| x |\n");

    let options = HtmlImportOptions {
        max_nodes: 100,
        ..Default::default()
    };
    assert!(matches!(
        html_to_ast(
            "<table><tr><td colspan=\"1000\">x</td></tr></table>",
            &options
        ),
        Err(HtmlImportError::NodeLimit)
    ));
}

/// A table arrives with two `<caption>` elements and Carve spells one, so one
/// goes either way. The parser keeps the first `^ ` line and reads the second
/// as a paragraph, so the import follows the same rule and says which one went.
#[test]
fn the_first_caption_wins_and_the_second_is_reported() {
    let html = "<table><caption>One</caption><tr><td>a</td></tr><caption>Two</caption></table>";
    let result = html_to_ast(html, &HtmlImportOptions::default()).unwrap();
    let (written, _) = round_trip(html);
    assert!(written.contains("^ One"), "{written}");
    assert!(!written.contains("Two"), "{written}");
    let reported = diagnostics(html);
    assert_eq!(reported.len(), 1, "{reported:?}");
    assert_eq!(
        reported[0].2,
        "Dropped a second <caption>: a table has one caption, and the first one wins"
    );
    assert!(
        result.report.diagnostics[0]
            .path
            .as_deref()
            .unwrap()
            .ends_with("/table[1]/caption[3]"),
        "{:?}",
        result.report.diagnostics[0].path
    );
}

/// CONTROL: ONE caption after the rows is where Carve writes it anyway, so it
/// is normalized rather than degraded.
#[test]
fn a_lone_caption_after_the_rows_is_not_a_degradation() {
    let html = "<table><tr><td>a</td></tr><caption>Late</caption></table>";
    let (written, _) = round_trip(html);
    assert_eq!(written, "| a |\n^ Late\n");
    assert_eq!(diagnostics(html), vec![]);
}

// The wall-clock guard that used to sit here - a tall table is read in time
// proportional to its rows - moved to `tests/perf_regressions.rs`, which CI
// runs alone and single-threaded (carve-rs#1092). It took ONE sample at 2000
// rows and one at 8000 and compared them, which is the shape that made `main`
// intermittently red on commits touching no engine code once the suite moved
// to a process-per-test runner. The claim is unchanged; only where it is
// measured is.
