//! A cell's `text-align` and `vertical-align` reach the cell's MARKER RUN in
//! `semantic` and `roundtrip`, and are dropped and reported in `safe`.
//!
//! `style` used to be refused wholesale, so a cell carrying `text-align:right`
//! came back unaligned AND carrying a `style-unmapped` row - a loss this engine
//! never had to take. The alignment had somewhere faithful to go the whole
//! time: a Carve cell alignment renders back as `style="text-align: right;"`,
//! the very declaration the import was handed, and `docs/html-import.md` makes
//! a declared loss a ceiling rather than a license (markup-carve/carve#1741).
//! `vertical-align` answers the same test through the cell's `valign` and was
//! not mapped either (markup-carve/carve#1746).
//!
//! THE DESTINATION IS THE NATIVE MARKER, NOT `{align=…}` / `{valign=…}`
//! (markup-carve/carve#1745). The two spellings do not render the same thing:
//! `|>` writes the CSS and `{align=right}` writes the presentational attribute,
//! so only the marker is a fixed point. With the key-value,
//! `carve -> html -> carve -> html` drifted, because the first render wrote the
//! declaration, the import turned it into the attribute, and the second render
//! wrote the attribute.
//!
//! THE BOUNDARY IS THE POINT, so every side of it is pinned here: the mapping
//! happens and SURVIVES A FULL RE-RENDER; `safe` still drops and still reports;
//! the properties and the values the language genuinely cannot spell still
//! report, so the change cannot read as a blanket "stop reporting"; and a body
//! cell repeating its column's value writes no run of its own, because the head
//! already says it.

use carve::{html_to_carve, to_html, HtmlImportDiagnosticCode, HtmlImportMode, HtmlImportOptions};

fn options(mode: HtmlImportMode) -> HtmlImportOptions {
    HtmlImportOptions {
        mode,
        ..Default::default()
    }
}

fn imported(html: &str, mode: HtmlImportMode) -> String {
    html_to_carve(html, &options(mode)).expect("import").value
}

fn codes(html: &str, mode: HtmlImportMode) -> Vec<HtmlImportDiagnosticCode> {
    html_to_carve(html, &options(mode))
        .expect("import")
        .report
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

fn cell(declaration: &str) -> String {
    format!("<table><tr><td style=\"{declaration}\">a</td><td>b</td></tr></table>")
}

/// THE CANARY. Every other assertion in this file is about the importer, and a
/// stale build serves an importer from before the edit while reporting a pass.
/// This one is the cheapest thing in the file that CANNOT hold unless the
/// binary linked the source this test shipped with, so a wrong-artifact run
/// fails here first and names itself instead of looking like a behavior bug.
#[test]
fn the_mapping_is_present_in_the_binary_under_test() {
    assert_eq!(
        imported(&cell("text-align:right"), HtmlImportMode::Semantic),
        "|> a | b |\n",
        "a stale artifact: this binary's importer predates the cell-alignment mapping"
    );
}

#[test]
fn text_align_reaches_the_marker_run_in_semantic_and_roundtrip() {
    for mode in [HtmlImportMode::Semantic, HtmlImportMode::Roundtrip] {
        for (value, marker) in [("right", ">"), ("left", "<"), ("center", "~")] {
            let html = cell(&format!("text-align:{value}"));
            assert_eq!(imported(&html, mode), format!("|{marker} a | b |\n"));
            assert_eq!(codes(&html, mode), Vec::new());
        }
    }
}

/// `?` STANDS FOR THE INHERITED HORIZONTAL. The vertical marker exists only in
/// the SECOND position of the run, so a cell stating only a vertical alignment
/// needs something in the first: a bare `|^` is not this spelling and comes
/// back as the literal text `^ a`, and `|~` alone is the CENTER horizontal
/// marker rather than a vertical one.
#[test]
fn vertical_align_reaches_the_run_behind_an_inherited_horizontal() {
    for mode in [HtmlImportMode::Semantic, HtmlImportMode::Roundtrip] {
        for (value, run) in [("top", "?^"), ("middle", "?~"), ("bottom", "?v")] {
            let html = cell(&format!("vertical-align:{value}"));
            assert_eq!(imported(&html, mode), format!("|{run} a | b |\n"));
            assert_eq!(codes(&html, mode), Vec::new());
        }
    }
}

/// Composed in ONE place and in one order, which is the only order that reads.
#[test]
fn both_axes_are_written_as_one_run() {
    for (declaration, run) in [
        ("text-align:right;vertical-align:top", ">^"),
        ("text-align:left;vertical-align:top", "<^"),
        ("text-align:right;vertical-align:bottom", ">v"),
        ("text-align:center;vertical-align:middle", "~~"),
    ] {
        let html = cell(declaration);
        assert_eq!(
            imported(&html, HtmlImportMode::Semantic),
            format!("|{run} a | b |\n")
        );
        assert_eq!(codes(&html, HtmlImportMode::Semantic), Vec::new());
    }
}

/// THE LOAD-BEARING ASSERTION. A test on the emitted Carve alone would pass for
/// a spelling no renderer reads, and which bytes come back out is the whole
/// reason the marker beats the key-value.
#[test]
fn the_re_render_gives_back_the_declaration_the_import_was_handed() {
    for (declaration, css) in [
        ("text-align:right", "text-align: right;"),
        ("text-align:left", "text-align: left;"),
        ("text-align:center", "text-align: center;"),
        ("vertical-align:top", "vertical-align: top;"),
        ("vertical-align:middle", "vertical-align: middle;"),
        ("vertical-align:bottom", "vertical-align: bottom;"),
        (
            "text-align:left;vertical-align:bottom",
            "text-align: left; vertical-align: bottom;",
        ),
    ] {
        let html = format!("<table><tr><td style=\"{declaration}\">a</td></tr></table>");
        let back = to_html(&imported(&html, HtmlImportMode::Semantic));
        assert!(
            back.contains(&format!("<td style=\"{css}\">a</td>")),
            "{declaration} re-rendered as {back}"
        );
    }
}

/// `carve -> html -> carve -> html` has to land on itself. It did not with the
/// key-value spelling, which is why markup-carve/carve#1745 is not a
/// preference.
#[test]
fn the_marker_run_is_a_fixed_point_through_html() {
    for source in [
        "|> a | b |\n",
        "|?^ a | b |\n",
        "|<^ a |\n",
        "|>v a |\n",
        "|~~ a |\n",
        "|=> h |\n| a |\n",
        "|=?^ h |\n| a |\n",
        "|=> h |= g |\n| a |> b |\n",
        "| a | b |\n| c |> d |\n",
    ] {
        let first = to_html(source);
        let back = imported(&first, HtmlImportMode::Roundtrip);
        assert_eq!(back, source, "carve -> html -> carve drifted");
        assert_eq!(to_html(&back), first, "the second render drifted");
    }
}

/// THE BOUNDARY A CARELESS FIX CROSSES. `safe` is the conservative mode and
/// maps no CSS onto a cell.
#[test]
fn safe_still_drops_the_alignment_and_still_reports_it() {
    for declaration in [
        "text-align:right",
        "text-align:left",
        "text-align:center",
        "vertical-align:top",
        "vertical-align:middle",
        "vertical-align:bottom",
        "text-align:right;vertical-align:top",
    ] {
        let html = cell(declaration);
        assert_eq!(imported(&html, HtmlImportMode::Safe), "| a | b |\n");
        assert_eq!(
            codes(&html, HtmlImportMode::Safe),
            vec![HtmlImportDiagnosticCode::StyleUnmapped]
        );
    }
}

/// THE CONTROL. Without it the change reads as a blanket "stop reporting".
/// Every one of these is a property with no Carve construct behind it: `width`
/// and `height` reach no per-cell slot even though a `TableColumn` carries a
/// width (markup-carve/carve#1092), and the rest have nothing at all.
#[test]
fn a_property_the_language_cannot_spell_still_reports() {
    for declaration in [
        "color:red",
        "background:blue",
        "width:50%",
        "height:2em",
        "border:1px solid",
        "padding:2px",
        "margin:0",
        "font-weight:bold",
        "white-space:nowrap",
    ] {
        let html = cell(declaration);
        assert_eq!(imported(&html, HtmlImportMode::Semantic), "| a | b |\n");
        assert_eq!(
            codes(&html, HtmlImportMode::Semantic),
            vec![HtmlImportDiagnosticCode::StyleUnmapped],
            "{declaration}"
        );
    }
}

/// A value outside Carve's enum is not quietly rounded to one that is. The
/// enums are `left` / `right` / `center` and `top` / `middle` / `bottom`, and
/// nothing else has a marker to write.
#[test]
fn a_value_outside_the_enum_still_reports() {
    for declaration in [
        "text-align:justify",
        "text-align:start",
        "text-align:end",
        "text-align:inherit",
        "vertical-align:baseline",
        "vertical-align:sub",
        "vertical-align:super",
        "vertical-align:4px",
    ] {
        let html = cell(declaration);
        assert_eq!(imported(&html, HtmlImportMode::Semantic), "| a | b |\n");
        assert_eq!(
            codes(&html, HtmlImportMode::Semantic),
            vec![HtmlImportDiagnosticCode::StyleUnmapped],
            "{declaration}"
        );
    }
}

/// A `style` carrying BOTH kinds maps the one it can and reports the one it
/// cannot, in one row - which is this engine's shape for the report and is what
/// keeps the mapping from silencing a real loss beside it.
#[test]
fn a_mapped_declaration_beside_an_unmapped_one_reports_only_the_loss() {
    let html = "<table><tr><td style=\"text-align:right;color:red\">a</td></tr></table>";
    assert_eq!(imported(html, HtmlImportMode::Semantic), "|> a |\n");
    assert_eq!(
        codes(html, HtmlImportMode::Semantic),
        vec![HtmlImportDiagnosticCode::StyleUnmapped]
    );
}

/// A `style` carrying NO declaration is not a loss and never was one. It
/// reported before only because the attribute was refused by name.
#[test]
fn a_style_attribute_with_nothing_in_it_reports_nothing() {
    for declaration in ["", ";;", "text-align", "   "] {
        let html = format!("<table><tr><td style=\"{declaration}\">a</td></tr></table>");
        assert_eq!(imported(&html, HtmlImportMode::Semantic), "| a |\n");
        assert_eq!(codes(&html, HtmlImportMode::Semantic), Vec::new());
    }
}

/// The declaration is read the way CSS is written, not the way one author
/// happened to type it.
#[test]
fn the_declaration_is_read_case_insensitively_and_untrimmed() {
    for declaration in [
        "TEXT-ALIGN: RIGHT",
        "text-align : right ;",
        "  text-align:Right  ",
    ] {
        let html = format!("<table><tr><td style=\"{declaration}\">a</td></tr></table>");
        assert_eq!(imported(&html, HtmlImportMode::Semantic), "|> a |\n");
    }
}

/// THE CASCADE. A browser reads the last declaration, and so does this.
#[test]
fn the_last_declaration_of_an_axis_wins() {
    let html = "<table><tr><td style=\"text-align:right;text-align:left\">a</td></tr></table>";
    assert_eq!(imported(html, HtmlImportMode::Semantic), "|< a |\n");
}

/// OFF A CELL there is no marker run, and the answer splits by property.
/// `align` is a legacy presentational attribute HTML defines for exactly these
/// elements, so the key-value is faithful. `valign` is defined for table cells
/// and nothing else, so writing it onto a paragraph would emit an attribute no
/// reader honors - a spelling that looks like a mapping and is not one.
#[test]
fn off_a_cell_only_the_horizontal_axis_maps() {
    assert_eq!(
        imported(
            "<p style=\"text-align:center\">x</p>",
            HtmlImportMode::Semantic
        ),
        "{align=center}\nx\n"
    );
    assert_eq!(
        codes(
            "<p style=\"text-align:center\">x</p>",
            HtmlImportMode::Semantic
        ),
        Vec::new()
    );

    assert_eq!(
        imported(
            "<p style=\"vertical-align:top\">x</p>",
            HtmlImportMode::Semantic
        ),
        "x\n"
    );
    assert_eq!(
        codes(
            "<p style=\"vertical-align:top\">x</p>",
            HtmlImportMode::Semantic
        ),
        vec![HtmlImportDiagnosticCode::StyleUnmapped]
    );

    // And `safe` maps nothing there either.
    assert_eq!(
        codes("<p style=\"text-align:center\">x</p>", HtmlImportMode::Safe),
        vec![HtmlImportDiagnosticCode::StyleUnmapped]
    );
}

/// A body cell repeating its column's value spells what the head already says.
/// The head IS the column default - the renderer reads it off the leading
/// header rows and every cell below inherits what it does not state - so a
/// round trip that wrote it again would grow a marker on every body row on its
/// first pass through HTML.
#[test]
fn a_body_cell_the_head_already_covers_writes_no_run() {
    let shared = "<table><thead><tr><th style=\"text-align:right\">h</th></tr></thead>\
                  <tbody><tr><td style=\"text-align:right\">a</td></tr></tbody></table>";
    assert_eq!(
        imported(shared, HtmlImportMode::Semantic),
        "|=> h |\n| a |\n"
    );

    // A cell that DISAGREES keeps its own run: that is the only thing that
    // overrides the default.
    let differing = "<table><thead><tr><th style=\"text-align:right\">h</th></tr></thead>\
                     <tbody><tr><td style=\"text-align:left\">a</td></tr></tbody></table>";
    assert_eq!(
        imported(differing, HtmlImportMode::Semantic),
        "|=> h |\n|< a |\n"
    );

    // A column with no default at all leaves every cell stating its own.
    let headless = "<table><thead><tr><th>h</th></tr></thead>\
                    <tbody><tr><td style=\"text-align:right\">a</td></tr></tbody></table>";
    assert_eq!(
        imported(headless, HtmlImportMode::Semantic),
        "|= h |\n|> a |\n"
    );

    // PER AXIS. A cell agreeing on the horizontal and stating its own vertical
    // keeps the vertical alone, which is what `?` exists to spell.
    let mixed = "<table><thead><tr><th style=\"text-align:right\">h</th></tr></thead>\
                 <tbody><tr><td style=\"text-align:right;vertical-align:top\">a</td></tr></tbody></table>";
    assert_eq!(
        imported(mixed, HtmlImportMode::Semantic),
        "|=> h |\n|?^ a |\n"
    );

    // And only the column the cell is IN. The second column's head states
    // nothing, so its body cell keeps its own run.
    let two = "<table><thead><tr><th style=\"text-align:right\">h</th><th>g</th></tr></thead>\
               <tbody><tr><td style=\"text-align:right\">a</td>\
               <td style=\"text-align:right\">b</td></tr></tbody></table>";
    assert_eq!(
        imported(two, HtmlImportMode::Semantic),
        "|=> h |= g |\n| a |> b |\n"
    );
}

/// THE COLUMN A CELL SITS IN IS THE ONE THE ROW'S CELL ARRAY PUTS IT IN. A
/// rowspan reaching down from the row above occupies an index, so the cells
/// after it shift right - and a walk that aged its own marks one row too early
/// would compare the row under the span against the wrong column and drop an
/// alignment the re-render cannot put back.
#[test]
fn a_rowspan_shifts_the_column_a_body_cell_is_compared_against() {
    // Column 0's head is right-aligned, column 1's states nothing. Every body
    // cell here sits in column 1, so every one of them keeps its run.
    let html = "<table><thead><tr><th style=\"text-align:right\">h</th><th>g</th></tr></thead>\
                <tbody><tr><td rowspan=\"3\">x</td><td style=\"text-align:right\">a</td></tr>\
                <tr><td style=\"text-align:right\">b</td></tr>\
                <tr><td style=\"text-align:right\">c</td></tr></tbody></table>";
    assert_eq!(
        imported(html, HtmlImportMode::Semantic),
        "|=> h |= g |\n| x |> a |\n| ^ |> b |\n| ^ |> c |\n"
    );
    // The mirror image: the head states the alignment on column 1, so every
    // body cell in it drops its run.
    let mirror = "<table><thead><tr><th>h</th><th style=\"text-align:right\">g</th></tr></thead>\
                  <tbody><tr><td rowspan=\"2\">x</td><td style=\"text-align:right\">a</td></tr>\
                  <tr><td style=\"text-align:right\">b</td></tr></tbody></table>";
    assert_eq!(
        imported(mirror, HtmlImportMode::Semantic),
        "|= h |=> g |\n| x | a |\n| ^ | b |\n"
    );
}

/// A HEADER CELL SEEDS ITS OWN COLUMN ONLY. This engine's renderer reads a
/// column default off the cell at that index, and the continuation cell a
/// colspan leaves at the next index states nothing - so seeding the span would
/// drop a body alignment the re-render could not put back. Asserted through the
/// re-render, because that is the fact the decision rests on.
#[test]
fn a_colspan_head_does_not_cover_the_columns_it_spans() {
    let html = "<table><thead><tr><th colspan=\"2\" style=\"text-align:right\">h</th></tr></thead>\
                <tbody><tr><td style=\"text-align:right\">a</td>\
                <td style=\"text-align:right\">b</td></tr></tbody></table>";
    let back = to_html(&imported(html, HtmlImportMode::Semantic));
    assert!(
        back.contains("<td style=\"text-align: right;\">a</td>")
            && back.contains("<td style=\"text-align: right;\">b</td>"),
        "an alignment was dropped that the re-render could not put back: {back}"
    );
}

/// CSS BEATS THE PRESENTATIONAL ATTRIBUTE, in both source orders. A browser
/// does not read `<td style="text-align:left" align="right">` as right-aligned
/// just because `align` was written second, so keeping both would spell one
/// axis twice, from one source, with the two disagreeing.
#[test]
fn a_presentational_attribute_a_declaration_supersedes_is_dropped() {
    for html in [
        "<table><tr><td style=\"text-align:left\" align=\"right\">a</td></tr></table>",
        "<table><tr><td align=\"right\" style=\"text-align:left\">a</td></tr></table>",
    ] {
        assert_eq!(imported(html, HtmlImportMode::Semantic), "|< a |\n");
        assert_eq!(
            codes(html, HtmlImportMode::Semantic),
            vec![HtmlImportDiagnosticCode::AttributeDropped]
        );
    }

    let vertical =
        "<table><tr><td style=\"vertical-align:top\" valign=\"bottom\">a</td></tr></table>";
    assert_eq!(imported(vertical, HtmlImportMode::Semantic), "|?^ a |\n");
    assert_eq!(
        codes(vertical, HtmlImportMode::Semantic),
        vec![HtmlImportDiagnosticCode::AttributeDropped]
    );
}

/// ONLY THE AXIS THE DECLARATION FILLS. A `valign` beside a `text-align` is not
/// superseded by it, and survives as the key-value it always was.
#[test]
fn a_presentational_attribute_on_the_other_axis_survives() {
    let html = "<table><tr><td style=\"text-align:right\" valign=\"top\">a</td></tr></table>";
    assert_eq!(
        imported(html, HtmlImportMode::Semantic),
        "|>{valign=top} a |\n"
    );
    assert_eq!(codes(html, HtmlImportMode::Semantic), Vec::new());

    let mirror = "<table><tr><td style=\"vertical-align:top\" align=\"right\">a</td></tr></table>";
    assert_eq!(
        imported(mirror, HtmlImportMode::Semantic),
        "|?^{align=right} a |\n"
    );
    assert_eq!(codes(mirror, HtmlImportMode::Semantic), Vec::new());
}

/// A PRESENTATIONAL ATTRIBUTE WITH NO CSS BESIDE IT was always kept, in every
/// mode, and the mapping must not start reporting it.
#[test]
fn a_bare_presentational_attribute_is_left_alone() {
    let html = "<table><tr><td align=\"right\">a</td></tr></table>";
    for mode in [
        HtmlImportMode::Safe,
        HtmlImportMode::Semantic,
        HtmlImportMode::Roundtrip,
    ] {
        assert_eq!(imported(html, mode), "|{align=right} a |\n");
        assert_eq!(codes(html, mode), Vec::new());
    }
}

/// The run comes AFTER the kind marker and BEFORE the attribute block, which is
/// the order the grammar binds them in (PART 9 §5 T10). Any other order reads
/// as content.
#[test]
fn the_run_composes_with_the_header_marker_and_an_attribute_block() {
    let html = "<table><tr><td style=\"text-align:right\" id=\"x\" class=\"k\">a</td></tr></table>";
    assert_eq!(imported(html, HtmlImportMode::Semantic), "|>{#x .k} a |\n");

    let header =
        "<table><tr><th style=\"text-align:center\">h</th></tr><tr><td>a</td></tr></table>";
    assert_eq!(
        imported(header, HtmlImportMode::Semantic),
        "|=~ h |\n| a |\n"
    );
}
