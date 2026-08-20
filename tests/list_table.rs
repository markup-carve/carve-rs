//! Golden-parity tests for the `ListTable` Tier-3 extension.
//!
//! Goldens captured from carve-php (`ListTableExtension`, PR #195) with
//! `(new CarveConverter())->addExtension(new ListTableExtension())->convert($src)`.
//! carve-rs's `<table>` layout is byte-identical to carve-php's, so the carve-rs
//! output matches the carve-php goldens verbatim.

use carve::{ListTable, Options};

#[test]
fn cell_alignment_overrides_column_defaults() {
    let html = h("{aligns=\"left,right\" valigns=\"top,bottom\"}\n::: list-table\n- -{align=center valign=middle} A\n  - B\n:::");
    assert!(html.contains("<td style=\"text-align: center; vertical-align: middle;\">A</td>"));
    assert!(html.contains("<td style=\"text-align: right; vertical-align: bottom;\">B</td>"));
    assert!(!html.contains(" align="));
    assert!(!html.contains(" valign="));
}

/// Render `src` with the list-table extension, trimmed.
fn h(src: &str) -> String {
    let ext = ListTable::new();
    let opts = Options::new().with_extension(&ext);
    carve::to_html_with_options(src, &opts).trim().to_string()
}

#[test]
fn basic_two_cell_row() {
    assert_eq!(
        h("::: list-table\n- - A\n  - B\n:::"),
        [
            "<table>",
            "  <tbody>",
            "    <tr><td>A</td><td>B</td></tr>",
            "  </tbody>",
            "</table>",
        ]
        .join("\n")
    );
}

#[test]
fn quoted_title_becomes_caption() {
    assert_eq!(
        h("::: list-table \"Cap\"\n- - A\n  - B\n:::"),
        [
            "<table>",
            "  <caption>Cap</caption>",
            "  <tbody>",
            "    <tr><td>A</td><td>B</td></tr>",
            "  </tbody>",
            "</table>",
        ]
        .join("\n")
    );
}

#[test]
fn caption_renders_inline_markup_in_title() {
    assert!(h("::: list-table \"Q *totals* `2026`\"\n- - A\n:::")
        .contains("<caption>Q <strong>totals</strong> <code>2026</code></caption>"));
}

#[test]
fn caption_renders_image_only_title() {
    assert!(h("::: list-table \"![alt](/x.png)\"\n- - A\n:::")
        .contains("<caption><img src=\"/x.png\" alt=\"alt\"></caption>"));
}

#[test]
fn empty_title_emits_no_caption() {
    let out = h("::: list-table \"\"\n- - A\n:::");
    assert!(!out.contains("<caption>"), "{out}");
}

#[test]
fn caption_escapes_html_special_chars() {
    assert!(h("::: list-table \"Tom & Jerry\"\n- - A\n  - B\n:::")
        .contains("<caption>Tom &amp; Jerry</caption>"));
}

#[test]
fn grouping_label_surfaces_as_caption_floor() {
    // A grouping `[label]` on a list-table must not be silently dropped when the
    // extension consumes the block: it surfaces as the same `<p class="div-label">`
    // the core caption floor would emit, after the title `<caption>`.
    assert_eq!(
        h("::: list-table \"Cap\" [Lbl]\n- - A\n:::"),
        [
            "<table>",
            "  <caption>Cap</caption>",
            "  <p class=\"div-label\">Lbl</p>",
            "  <tbody>",
            "    <tr><td>A</td></tr>",
            "  </tbody>",
            "</table>",
        ]
        .join("\n")
    );
    // The label is escaped.
    assert!(h("::: list-table [<b>x</b>]\n- - A\n:::")
        .contains("<p class=\"div-label\">&lt;b&gt;x&lt;/b&gt;</p>"));
}

#[test]
fn header_rows_promote_to_thead() {
    assert_eq!(
        h("{header-rows=1}\n::: list-table\n- - Region\n  - Q1\n- - EMEA\n  - 10\n:::"),
        [
            "<table>",
            "  <thead>\n    <tr><th scope=\"col\">Region</th><th scope=\"col\">Q1</th></tr>\n  </thead>",
            "  <tbody>",
            "    <tr><td>EMEA</td><td>10</td></tr>",
            "  </tbody>",
            "</table>",
        ]
        .join("\n")
    );
}

#[test]
fn boolean_header_rows_promotes_first_row() {
    // `{header-rows}` with no value is the boolean form: the first row is the
    // header, the default a table with headers wants.
    assert_eq!(
        h("{header-rows}\n::: list-table\n- - Region\n  - Q1\n- - EMEA\n  - 10\n:::"),
        [
            "<table>",
            "  <thead>\n    <tr><th scope=\"col\">Region</th><th scope=\"col\">Q1</th></tr>\n  </thead>",
            "  <tbody>",
            "    <tr><td>EMEA</td><td>10</td></tr>",
            "  </tbody>",
            "</table>",
        ]
        .join("\n")
    );
}

#[test]
fn boolean_header_cols_promotes_first_column() {
    assert_eq!(
        h("{header-cols}\n::: list-table\n- - Region\n  - Q1\n- - EMEA\n  - 10\n:::"),
        [
            "<table>",
            "  <tbody>",
            "    <tr><th scope=\"row\">Region</th><td>Q1</td></tr>",
            "    <tr><th scope=\"row\">EMEA</th><td>10</td></tr>",
            "  </tbody>",
            "</table>",
        ]
        .join("\n")
    );
}

#[test]
fn header_cols_promote_first_cell_to_row_header() {
    assert_eq!(
        h("{header-cols=1}\n::: list-table\n- - Region\n  - Q1\n- - EMEA\n  - 10\n:::"),
        [
            "<table>",
            "  <tbody>",
            "    <tr><th scope=\"row\">Region</th><td>Q1</td></tr>",
            "    <tr><th scope=\"row\">EMEA</th><td>10</td></tr>",
            "  </tbody>",
            "</table>",
        ]
        .join("\n")
    );
}

#[test]
fn header_rows_and_cols_combine() {
    assert_eq!(
        h("{header-rows=1}\n{header-cols=1}\n::: list-table\n- - Region\n  - Q1\n- - EMEA\n  - 10\n:::"),
        [
            "<table>",
            "  <thead>\n    <tr><th scope=\"col\">Region</th><th scope=\"col\">Q1</th></tr>\n  </thead>",
            "  <tbody>",
            "    <tr><th scope=\"row\">EMEA</th><td>10</td></tr>",
            "  </tbody>",
            "</table>",
        ]
        .join("\n")
    );
}

#[test]
fn footer_rows_render_one_per_line() {
    assert_eq!(
        h("{footer-rows=2}\n{header-cols=1}\n::: list-table\n- - Region\n  - Q1\n- - EMEA\n  - 10\n- - Region\n  - Q1\n- - EMEA\n  - 10\n:::"),
        [
            "<table>",
            "  <tbody>",
            "    <tr><th scope=\"row\">Region</th><td>Q1</td></tr>",
            "    <tr><th scope=\"row\">EMEA</th><td>10</td></tr>",
            "  </tbody>",
            "  <tfoot>",
            "    <tr><th scope=\"row\">Region</th><td>Q1</td></tr>",
            "    <tr><th scope=\"row\">EMEA</th><td>10</td></tr>",
            "  </tfoot>",
            "</table>",
        ]
        .join("\n")
    );
}

#[test]
fn block_cell_keeps_wrappers() {
    // A multi-block cell (paragraph + list) keeps its block wrappers; the
    // single-cell A collapses to inline content.
    assert_eq!(
        h("::: list-table\n- - A\n  - Strong quarter.\n\n    - new logos\n    - renewals\n:::"),
        [
            "<table>",
            "  <tbody>",
            "    <tr><td>A</td><td><p>Strong quarter.</p>",
            "<ul>",
            "  <li>new logos</li>",
            "  <li>renewals</li>",
            "</ul></td></tr>",
            "  </tbody>",
            "</table>",
        ]
        .join("\n")
    );
}

#[test]
fn sales_example_rowspan_and_colspan() {
    // The reference Sales example: EMEA gets rowspan="2" (it plus the `^`
    // below); Total gets colspan="3" (it plus the two `<`).
    assert_eq!(
        h("{header-rows=1}\n::: list-table \"Sales\"\n- - Region\n  - Q1\n  - Q2\n- - EMEA\n  - 10\n  - 12\n- - ^\n  - 14\n  - 16\n- - Total\n  - <\n  - <\n:::"),
        [
            "<table>",
            "  <caption>Sales</caption>",
            "  <thead>\n    <tr><th scope=\"col\">Region</th><th scope=\"col\">Q1</th><th scope=\"col\">Q2</th></tr>\n  </thead>",
            "  <tbody>",
            "    <tr><td rowspan=\"2\">EMEA</td><td>10</td><td>12</td></tr>",
            "    <tr><td>14</td><td>16</td></tr>",
            "    <tr><td colspan=\"3\">Total</td></tr>",
            "  </tbody>",
            "</table>",
        ]
        .join("\n")
    );
}

#[test]
fn lone_caret_escaped_by_cell_attribute_is_literal() {
    // A cell carrying its own attribute is never a span marker: its `^` stays
    // literal and the cell attribute carries onto the <td>.
    assert_eq!(
        h("::: list-table\n- - A\n  -{.x} ^\n:::"),
        [
            "<table>",
            "  <tbody>",
            "    <tr><td>A</td><td class=\"x\">^</td></tr>",
            "  </tbody>",
            "</table>",
        ]
        .join("\n")
    );
}

#[test]
fn cell_attributes_carry_onto_td() {
    assert_eq!(
        h("::: list-table\n- - A\n  -{.hi} B\n:::"),
        [
            "<table>",
            "  <tbody>",
            "    <tr><td>A</td><td class=\"hi\">B</td></tr>",
            "  </tbody>",
            "</table>",
        ]
        .join("\n")
    );
}

#[test]
fn ragged_rows_padded_with_empty_cells() {
    assert_eq!(
        h("::: list-table\n- - A\n  - B\n  - C\n- - D\n:::"),
        [
            "<table>",
            "  <tbody>",
            "    <tr><td>A</td><td>B</td><td>C</td></tr>",
            "    <tr><td>D</td><td></td><td></td></tr>",
            "  </tbody>",
            "</table>",
        ]
        .join("\n")
    );
}

#[test]
fn row_with_no_cell_list_defers_to_plain_div() {
    // A row authored as a plain paragraph (no inner cell list) cannot be
    // rendered as cells, so the whole block degrades to the literal nested-list
    // div and nothing is dropped.
    assert_eq!(
        h("::: list-table\n- - A\n  - B\n- not a cell row\n:::"),
        [
            "<div class=\"list-table\">",
            "  <ul>",
            "    <li>",
            "      <ul>",
            "        <li>A</li>",
            "        <li>B</li>",
            "      </ul>",
            "    </li>",
            "    <li>not a cell row</li>",
            "  </ul>",
            "</div>",
        ]
        .join("\n")
    );
}

#[test]
fn header_rowspan_clamped_at_thead_tbody_boundary() {
    // A `^` in a body row whose origin sits in the header rows finds no valid
    // origin (an HTML cell cannot span <thead> into <tbody>) and degrades to an
    // empty cell.
    assert_eq!(
        h("{header-rows=1}\n::: list-table\n- - H1\n  - H2\n- - ^\n  - B2\n:::"),
        [
            "<table>",
            "  <thead>\n    <tr><th scope=\"col\">H1</th><th scope=\"col\">H2</th></tr>\n  </thead>",
            "  <tbody>",
            "    <tr><td></td><td>B2</td></tr>",
            "  </tbody>",
            "</table>",
        ]
        .join("\n")
    );
}

#[test]
fn cell_content_escapes_html() {
    assert_eq!(
        h("::: list-table\n- - Tom & <b>Jerry</b>\n  - x\n:::"),
        [
            "<table>",
            "  <tbody>",
            "    <tr><td>Tom &amp; &lt;b&gt;Jerry&lt;/b&gt;</td><td>x</td></tr>",
            "  </tbody>",
            "</table>",
        ]
        .join("\n")
    );
}

#[test]
fn no_inner_list_defers_to_plain_div() {
    assert_eq!(
        h("::: list-table\njust a paragraph\n:::"),
        [
            "<div class=\"list-table\">",
            "  <p>just a paragraph</p>",
            "</div>",
        ]
        .join("\n")
    );
}

#[test]
fn table_attributes_carry_onto_table_tag() {
    // A preceding attribute line carries id / sibling classes onto the <table>;
    // the structural header-rows / header-cols keys are consumed and dropped.
    assert_eq!(
        h("{#t1 .striped header-rows=1}\n::: list-table\n- - A\n  - B\n- - C\n  - D\n:::"),
        [
            "<table id=\"t1\" class=\"striped\">",
            "  <thead>\n    <tr><th scope=\"col\">A</th><th scope=\"col\">B</th></tr>\n  </thead>",
            "  <tbody>",
            "    <tr><td>C</td><td>D</td></tr>",
            "  </tbody>",
            "</table>",
        ]
        .join("\n")
    );
}

#[test]
fn leading_colspan_marker_becomes_empty_cell() {
    // A leading `<` (no content cell to its left to merge into) becomes its own
    // empty cell rather than being dropped (pipe-table parity).
    assert_eq!(
        h("::: list-table\n- - <\n  - B\n:::"),
        [
            "<table>",
            "  <tbody>",
            "    <tr><td></td><td>B</td></tr>",
            "  </tbody>",
            "</table>",
        ]
        .join("\n")
    );
}

#[test]
fn blocked_colspan_marker_becomes_empty_cell() {
    // Row 2's `<` (column 2) has no available origin to its left: column 1 is
    // held by A's rowspan (the `^` below A), so the `<` cannot merge and renders
    // as an empty cell rather than being dropped (which would shift `D` left).
    // Matches carve-js and the equivalent pipe table.
    assert_eq!(
        h("::: list-table\n- - A\n  - B\n  - C\n- - ^\n  - <\n  - D\n:::"),
        [
            "<table>",
            "  <tbody>",
            "    <tr><td rowspan=\"2\">A</td><td>B</td><td>C</td></tr>",
            "    <tr><td></td><td>D</td></tr>",
            "  </tbody>",
            "</table>",
        ]
        .join("\n")
    );
}

#[test]
fn first_row_caret_then_colspan_marker_merge_into_empty_cell() {
    // A first-row `^` is an unmergeable empty cell; a `<` immediately to its
    // right merges INTO that empty cell, growing its colspan (the `<` has a valid
    // non-skipped left neighbor). Matches carve-js.
    assert_eq!(
        h("::: list-table\n- - ^\n  - <\n  - B\n:::"),
        [
            "<table>",
            "  <tbody>",
            "    <tr><td colspan=\"2\"></td><td>B</td></tr>",
            "  </tbody>",
            "</table>",
        ]
        .join("\n")
    );
}

#[test]
fn first_row_caret_becomes_empty_cell() {
    // A `^` in the first row has no cell above to extend, so it is an empty cell.
    assert_eq!(
        h("::: list-table\n- - ^\n  - B\n:::"),
        [
            "<table>",
            "  <tbody>",
            "    <tr><td></td><td>B</td></tr>",
            "  </tbody>",
            "</table>",
        ]
        .join("\n")
    );
}

#[test]
fn without_extension_stays_plain_div() {
    assert!(carve::to_html("::: list-table \"Cap\"\n- - A\n  - B\n:::")
        .contains("<div class=\"list-table\">"));
}

#[test]
fn leaves_other_custom_admonitions_untouched() {
    assert!(h("::: aside-note\nhi\n:::").contains("<div class=\"aside-note\">"));
}

#[test]
fn restrictive_profile_gates_list_table_as_a_div() {
    // The rewrite happens before profile filtering, but the carrier is gated as
    // a `div` (its origin), so a profile that denies custom containers strips
    // the table exactly as it would the underlying admonition.
    use carve::Profile;
    let src = "::: list-table \"Cap\"\n- - A\n  - B\n:::";
    let ext = ListTable::new();
    let no_ext = carve::to_html_with_options(src, &Options::new().with_profile(Profile::comment()));
    let with_ext = carve::to_html_with_options(
        src,
        &Options::new()
            .with_extension(&ext)
            .with_profile(Profile::comment()),
    );
    assert_eq!(
        with_ext, no_ext,
        "list-table must not bypass the div restriction"
    );
}
