//! PART 12 §3's `separatorWidths`: the dash runs an author's separator row
//! spelled, recorded on the table, published on the wire and spelled back
//! (markup-carve/carve#2894, carriers named on markup-carve/carve#2902).
//!
//! PRESENCE is the larger half. `|= a |= b |` and a plain row over `|---|---|`
//! give the same rows and the same alignments, so the rows cannot say which the
//! author wrote; absence says NO separator row and is what the `|=` form
//! spells.

use carve::ast::{BlockNode, Table};
use carve::{
    from_json, html_to_carve, parse, render_carve, to_html_with_options, to_json, HtmlImportMode,
    HtmlImportOptions, Options,
};

const ALIGNED: &str = "| a | b |\n|:-----|---:|\n| 1 | 2 |\n";

fn table_of(source: &str) -> Table {
    match parse(source).children.first() {
        Some(BlockNode::Table(table)) => table.clone(),
        other => panic!("expected a table, got {other:?}"),
    }
}

fn spelled(source: &str) -> String {
    render_carve(&parse(source)).expect("the table spells")
}

fn through_json(source: &str) -> String {
    let json = to_json(&parse(source));
    render_carve(&from_json(&json).expect("the payload reads")).expect("the table spells")
}

// ------------------------------------------------------------------ the parser

#[test]
fn the_parser_records_the_dash_run_of_each_separator_cell() {
    assert_eq!(
        table_of(ALIGNED).separator_widths,
        Some(vec![5, 3]),
        "the colons are not dashes and do not count"
    );
    assert_eq!(
        table_of("| a | b |\n|---|-------|\n").separator_widths,
        Some(vec![3, 7])
    );
    assert_eq!(
        table_of("| a |\n|-|\n").separator_widths,
        Some(vec![1]),
        "a delimiter cell needs one dash, so no recorded width is below 1"
    );
    assert_eq!(
        table_of("| a | b |\n| :---: | - |\n").separator_widths,
        Some(vec![3, 1]),
        "cell padding is not content"
    );
}

#[test]
fn a_native_header_records_no_separator_row_at_all() {
    assert_eq!(table_of("|= a |= b |\n| 1 | 2 |\n").separator_widths, None);
    assert_eq!(table_of("| 1 | 2 |\n").separator_widths, None);
}

#[test]
fn the_two_header_spellings_differ_only_in_this_field() {
    // The point of the field: everything else about the two parses agrees.
    let separated = table_of("| a | b |\n|---|---|\n| 1 | 2 |\n");
    let native = table_of("|= a |= b |\n| 1 | 2 |\n");
    assert_eq!(separated.rows, native.rows);
    assert_eq!(separated.separator_widths, Some(vec![3, 3]));
    assert_eq!(native.separator_widths, None);
}

// -------------------------------------------------------------------- the wire

#[test]
fn the_field_rides_the_wire_as_integers() {
    assert!(
        to_json(&parse(ALIGNED)).contains(r#""separatorWidths":[5,3]"#),
        "{}",
        to_json(&parse(ALIGNED))
    );
    assert!(!to_json(&parse("|= a |\n")).contains("separatorWidths"));
}

#[test]
fn the_payload_reads_back_to_the_same_tree() {
    for source in [
        ALIGNED,
        "| a | b |\n|---|---|\n| 1 | 2 |\n",
        "|= a |= b |\n| 1 | 2 |\n",
    ] {
        let json = to_json(&parse(source));
        let read = from_json(&json).expect("the payload reads");
        assert_eq!(to_json(&read), json, "{source:?}");
    }
}

#[test]
fn the_closed_schema_refuses_a_width_that_is_not_a_positive_integer() {
    let payload = |widths: &str| {
        format!(
            r#"{{"type":"document","srcByteLength":0,"children":[{{"type":"table","rows":[{{"type":"table_row","cells":[{{"type":"table_cell","header":true,"children":[]}}]}}],"separatorWidths":{widths}}}]}}"#
        )
    };
    for widths in ["[0]", "[-1]", "[1.5]", r#"["3"]"#, "[null]", "3"] {
        from_json(&payload(widths)).expect_err(&format!("{widths} is not a separator width list"));
    }
    from_json(&payload("[1]")).expect("one dash is the smallest run there is");
    from_json(&payload("[]")).expect("presence with no cell is still presence");
}

// ------------------------------------------------------------------ the writer

#[test]
fn the_writer_spells_the_separator_row_the_author_wrote() {
    assert_eq!(spelled(ALIGNED), ALIGNED);
    assert_eq!(through_json(ALIGNED), ALIGNED);
    let plain = "| a | b |\n|---|-------|\n| 1 | 2 |\n";
    assert_eq!(spelled(plain), plain);
}

#[test]
fn absence_still_spells_the_native_header_form() {
    assert_eq!(
        spelled("|= a |= b |\n| 1 | 2 |\n"),
        "|= a |= b |\n| 1 | 2 |\n"
    );
    // The alignment has nowhere else to go, so it stays on the cells.
    assert_eq!(spelled("|=< a |=> b |\n"), "|=< a |=> b |\n");
}

#[test]
fn a_width_below_one_is_written_as_one() {
    // A parse cannot record a 0 and the wire refuses one, so this is a tree
    // built in memory - the one place the value can arrive.
    let mut table = table_of(ALIGNED);
    table.separator_widths = Some(vec![0, 0]);
    let mut doc = parse(ALIGNED);
    doc.children = vec![BlockNode::Table(table)];
    assert_eq!(
        render_carve(&doc).expect("the table spells"),
        "| a | b |\n|:-|-:|\n| 1 | 2 |\n"
    );
}

#[test]
fn an_entry_count_that_disagrees_is_spelled_for_the_cells_that_exist() {
    let mut doc = parse(ALIGNED);
    let mut table = table_of(ALIGNED);
    table.separator_widths = Some(vec![9]);
    doc.children = vec![BlockNode::Table(table.clone())];
    assert_eq!(
        render_carve(&doc).expect("the table spells"),
        "| a | b |\n|:---------|---:|\n| 1 | 2 |\n",
        "a cell past the entries takes the default run"
    );
    table.separator_widths = Some(vec![1, 1, 1, 1]);
    doc.children = vec![BlockNode::Table(table)];
    assert_eq!(
        render_carve(&doc).expect("the table spells"),
        "| a | b |\n|:-|-:|\n| 1 | 2 |\n",
        "and no further than the cells the row has"
    );
}

// ------------------------------------------------------- the HTML round trip

fn exported(source: &str, roundtrip: bool) -> String {
    to_html_with_options(source, &Options::default().with_roundtrip(roundtrip))
}

fn imported(html: &str) -> String {
    html_to_carve(
        html,
        &HtmlImportOptions {
            mode: HtmlImportMode::Safe,
            ..HtmlImportOptions::default()
        },
    )
    .expect("the import holds")
    .value
}

#[test]
fn roundtrip_export_carries_the_widths_and_the_import_comes_back_byte_identical() {
    let html = exported(ALIGNED, true);
    assert!(
        html.contains(r#"<table data-carve-col-widths="5,3">"#),
        "{html}"
    );
    assert_eq!(imported(&html), ALIGNED);
}

#[test]
fn with_the_option_off_the_table_still_comes_back_as_the_native_form() {
    let html = exported(ALIGNED, false);
    assert!(!html.contains("col-widths"), "{html}");
    assert_eq!(imported(&html), "|=< a |=> b |\n| 1 | 2 |\n");
}

#[test]
fn a_native_header_carries_nothing_and_its_bytes_do_not_move() {
    let source = "|= a |= b |\n| 1 | 2 |\n";
    assert_eq!(exported(source, true), exported(source, false));
}
