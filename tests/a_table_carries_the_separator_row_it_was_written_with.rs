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
    from_json, from_prosemirror, html_to_carve, parse, render_carve, to_html_with_options, to_json,
    to_prosemirror, HtmlImportMode, HtmlImportOptions, Options,
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
    for widths in [
        "[0]", "[-1]", "[1.5]", r#"["3"]"#, "[null]", "3", "[]", "null",
    ] {
        from_json(&payload(widths)).expect_err(&format!("{widths} is not a separator width list"));
    }
    from_json(&payload("[1]")).expect("one dash is the smallest run there is");
    from_json(&payload("[4294967296]")).expect("the field is not bounded to u32");
    let overflow = from_json(&payload("[18446744073709551616]")).unwrap_err();
    assert!(overflow.to_string().contains("representable as u64"));
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
        "| a |> b |\n|:---------|\n| 1 | 2 |\n",
        "one separator cell per entry, and a column past them keeps its own marker"
    );
    table.separator_widths = Some(vec![1, 1, 1, 1]);
    doc.children = vec![BlockNode::Table(table)];
    assert_eq!(
        render_carve(&doc).expect("the table spells"),
        "| a | b |\n|:-|-:|-|-|\n| 1 | 2 |\n",
        "an entry past the row's cells has no alignment to take"
    );
}

#[test]
fn spelling_one_cell_per_entry_keeps_the_writer_a_fixed_point() {
    // Filling the ROW's width instead invented an entry the author never
    // wrote, so the second format moved the tree.
    for source in [
        "| |x |\n|---|\n| y |\n",
        "| h |\n|---|\n| |x |\n",
        "| a | b |\n|---|\n| 1 | 2 |\n",
        ALIGNED,
    ] {
        let once = spelled(source);
        assert_eq!(spelled(&once), once, "not idempotent for {source:?}");
        // The byte length moves with the spelling; the TREE must not.
        assert_eq!(
            table_of(&once),
            table_of(source),
            "the tree moved for {source:?}"
        );
    }
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
fn the_import_refuses_the_whole_carrier_rather_than_reading_part_of_it() {
    // PART 10 §13a CARVE-P10-016: not a run of decimal digits, or past 1000,
    // refuses the WHOLE carrier - no field, so the `|=` form. Neither a partial
    // read nor a clamp: both would put a separator row on the page whose widths
    // nobody wrote. The bound exists because the value arrives in an attribute
    // off untrusted HTML and the writer spells one `-` per unit, so six
    // characters would buy a megabyte of separator row. 1000 is pinned for
    // cross-engine parity rather than derived.
    for value in [
        "x,2",
        "2,x",
        "-1,2",
        "1.5",
        "2,",
        " ",
        "1e3",
        "1001",
        "2,1001",
        "4000",
        "99999999999999999999",
        "0,2",
    ] {
        let html = format!(
            r#"<table data-carve-col-widths="{value}"><tr><th>a</th><th>b</th></tr></table>"#
        );
        let back = imported(&html);
        assert_eq!(back, "|= a |= b |\n", "{value:?}");
        assert!(!back.contains("col-widths"), "{value:?}");
    }
    // The bound itself still reads.
    let at_limit = r#"<table data-carve-col-widths="1000,2"><tr><th>a</th><th>b</th></tr></table>"#;
    assert_eq!(
        imported(at_limit),
        format!("| a | b |\n|{}|--|\n", "-".repeat(1000))
    );
}

#[test]
fn the_export_writes_no_carrier_for_a_field_a_reader_would_refuse() {
    // A carrier a conforming reader refuses is a round-trip promise the writer
    // cannot keep. The FIELD has no upper bound of its own: a parse records
    // what the author typed, and nothing amplifies there.
    let mut doc = parse(ALIGNED);
    let mut table = table_of(ALIGNED);
    table.separator_widths = Some(vec![1001, 3]);
    doc.children = vec![BlockNode::Table(table.clone())];
    let html = carve::render_html_with_options(&doc, &Options::default().with_roundtrip(true))
        .expect("the table renders");
    assert!(!html.contains("col-widths"), "{html}");
    table.separator_widths = Some(vec![1000, 3]);
    doc.children = vec![BlockNode::Table(table)];
    let html = carve::render_html_with_options(&doc, &Options::default().with_roundtrip(true))
        .expect("the table renders");
    assert!(html.contains(r#"data-carve-col-widths="1000,3""#), "{html}");
}

#[test]
fn the_entry_count_decides_the_cell_count_in_both_directions() {
    // PART 11 §6h CARVE-P11-066: the cell count is the number of ENTRIES, and
    // the header row does not enter it - neither padded out to the header's
    // width nor truncated to it. The two directions are tested TOGETHER on
    // purpose: where the separator is SHORTER than its header, padding and
    // truncating give the same bytes, which is how carve-php (padding) and
    // carve-js (truncating) each passed a suite that only covered that side.
    //
    // Asserted on the TREE, not on the line: a writer that pads or truncates
    // still emits a plausible table, so byte equality alone cannot see it.
    for (source, widths) in [
        // separator LONGER than its header - the half that catches truncation
        ("| a | b |\n|---|---|---|\n| 1 | 2 |\n", vec![3, 3, 3]),
        ("| a |\n|---|---|\n| 1 |\n", vec![3, 3]),
        // separator SHORTER than its header - the half that catches padding
        ("| a | b |\n|---|\n| 1 | 2 |\n", vec![3]),
        ("| a | b | c |\n|--|--|\n| 1 | 2 | 3 |\n", vec![2, 2]),
    ] {
        assert_eq!(
            table_of(source).separator_widths,
            Some(widths.clone()),
            "the premise for {source:?}"
        );
        let once = spelled(source);
        assert_eq!(
            table_of(&once).separator_widths,
            Some(widths),
            "the re-parse disagrees for {source:?}; written as {once:?}"
        );
        assert_eq!(table_of(&once), table_of(source), "{source:?}");
        assert_eq!(spelled(&once), once, "not idempotent for {source:?}");
    }
}

#[test]
fn an_entry_past_the_headers_last_cell_takes_no_alignment() {
    // It has no cell to take one from, which falls straight out of the entry
    // count deciding the row.
    let source = "| a |\n|:---:|---|\n| 1 |\n";
    assert_eq!(table_of(source).separator_widths, Some(vec![3, 3]));
    assert_eq!(spelled(source), source);
}

// ------------------------------------------------- the ProseMirror bridge

#[test]
fn the_prosemirror_bridge_carries_the_authored_separator_row() {
    // The bridge has no table slot of its own for it, so it rides
    // `carveSeparatorWidths` the way an authored bullet rides
    // `carveListMarker`. Without that the round trip rewrote 19 corpus tables
    // into the `|=` form.
    for source in [ALIGNED, "| a | b |\n|---|---|\n| 1 | 2 |\n"] {
        let pm = to_prosemirror(&parse(source)).json;
        let back = from_prosemirror(&pm).expect("the bridge reads its own output");
        assert_eq!(
            render_carve(&back).expect("it spells"),
            source,
            "{source:?}"
        );
    }
    // A native header carries no attribute to come back with.
    let pm = to_prosemirror(&parse("|= a |= b |\n")).json;
    assert!(!pm.contains("carveSeparatorWidths"), "{pm}");
}

#[test]
fn a_native_header_carries_nothing_and_its_bytes_do_not_move() {
    let source = "|= a |= b |\n| 1 | 2 |\n";
    assert_eq!(exported(source, true), exported(source, false));
}

#[test]
fn a_separator_without_a_preceding_row_has_a_typed_source_refusal() {
    let doc = from_json(
        r#"{"type":"document","srcByteLength":0,"children":[{"type":"table","rows":[],"separatorWidths":[3]}]}"#,
    )
    .expect("the wire permits a table without rows");
    match render_carve(&doc).expect_err("a separator cannot open a table") {
        carve::RenderCarveError::SourceUnspellable(error) => {
            assert_eq!(error.node_type(), "table");
            assert_eq!(
                error.reason(),
                "a separator row needs a preceding table row"
            );
        }
        other => panic!("expected a source refusal, got {other:?}"),
    }
}

#[test]
fn large_widths_remain_exact_on_the_wire_and_refuse_unallocatable_source() {
    for widths in [
        vec![u64::MAX],
        vec![isize::MAX as u64, 1],
        vec![u64::MAX / 2, u64::MAX / 2, 2],
    ] {
        let mut doc = parse(ALIGNED);
        let Some(BlockNode::Table(table)) = doc.children.first_mut() else {
            unreachable!()
        };
        table.separator_widths = Some(widths);
        let json = to_json(&doc);
        assert_eq!(to_json(&from_json(&json).unwrap()), json);
        match render_carve(&doc).expect_err("source capacity must be checked before allocation") {
            carve::RenderCarveError::SourceUnspellable(error) => {
                assert_eq!(error.node_type(), "table");
                assert_eq!(
                    error.reason(),
                    "separator row exceeds source allocation capacity"
                );
            }
            other => panic!("expected source refusal, got {other:?}"),
        }
    }
}

#[test]
fn separator_widths_do_not_promote_a_non_header_row() {
    let html = r#"<table data-carve-col-widths="3"><tr><td>a</td></tr><tr><td>b</td></tr></table>"#;
    let opts = HtmlImportOptions {
        mode: HtmlImportMode::Safe,
        ..HtmlImportOptions::default()
    };
    let doc = carve::html_to_ast(html, &opts).unwrap().value;
    let Some(BlockNode::Table(table)) = doc.children.first() else {
        unreachable!()
    };
    assert_eq!(table.separator_widths, Some(vec![3]));
    assert!(!table.rows[0].cells[0].header);
    match render_carve(&doc).expect_err("a separator must not change td into th") {
        carve::RenderCarveError::SourceUnspellable(error) => {
            assert_eq!(error.node_type(), "table");
            assert_eq!(error.reason(), "field separatorWidths needs a header row");
        }
        other => panic!("expected source refusal, got {other:?}"),
    }
    assert_eq!(
        carve::html_to_carve(html, &opts).unwrap_err(),
        carve::HtmlImportError::SourceUnspellable
    );
}

#[test]
fn prosemirror_separator_widths_use_decimal_digits_without_the_html_policy_bound() {
    let with_widths = |widths: &str| {
        let mut pm: serde_json::Value =
            serde_json::from_str(&to_prosemirror(&parse(ALIGNED)).json).unwrap();
        pm["content"][0]["attrs"]["carveSeparatorWidths"] =
            serde_json::Value::String(widths.to_owned());
        from_prosemirror(&pm.to_string()).unwrap()
    };
    for invalid in ["+3", "0", "", "2,", "2,x", "1.5", "18446744073709551616"] {
        let doc = with_widths(invalid);
        let Some(BlockNode::Table(table)) = doc.children.first() else {
            unreachable!()
        };
        assert_eq!(table.separator_widths, None, "{invalid:?}");
    }
    for (value, width) in [
        ("1001", 1001),
        ("4294967296", 4294967296),
        ("18446744073709551615", u64::MAX),
    ] {
        let doc = with_widths(value);
        let Some(BlockNode::Table(table)) = doc.children.first() else {
            unreachable!()
        };
        assert_eq!(table.separator_widths, Some(vec![width]));
    }
}

#[test]
fn invalid_runtime_width_lists_do_not_emit_an_html_carrier() {
    for widths in [vec![], vec![0, 3]] {
        let mut doc = parse(ALIGNED);
        let Some(BlockNode::Table(table)) = doc.children.first_mut() else {
            unreachable!()
        };
        table.separator_widths = Some(widths);
        let html = carve::render_html_with_options(&doc, &Options::default().with_roundtrip(true))
            .unwrap();
        assert!(!html.contains("col-widths"));
    }
}
