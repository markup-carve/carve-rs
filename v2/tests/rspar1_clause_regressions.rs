use carve2::Options;
use carve2::lint::{LintOptions, lint};

fn findings(source: &str, code: &str) -> Vec<carve2::diagnostic::Diagnostic> {
    lint(source, &LintOptions::default())
        .into_iter()
        .filter(|finding| finding.code == code)
        .collect()
}

#[test]
fn c2_b_022_list_marker_inside_term_stays_lazy() {
    assert_eq!(
        carve2::render("- :: a\n   - b", &Options::default()),
        "<ul>\n  <li>\n    <dl>\n      <dt>a\n- b</dt>\n    </dl>\n  </li>\n</ul>"
    );
}

#[test]
fn c2_b_119_space_then_tab_opens_anonymous_div() {
    assert_eq!(
        carve2::render("::: \tnote\nx\n:::", &Options::default()),
        "<div>\n  <p>x</p>\n</div>"
    );
}

#[test]
fn c2_w_107_escaped_mark_is_no_unclosed_emphasis() {
    assert_eq!(findings("a\\*b", "unclosed-inline-emphasis"), Vec::new());
    assert_eq!(findings("a\\/b", "unclosed-inline-emphasis"), Vec::new());
}

#[test]
fn c2_b_024_lazy_heading_is_no_overindented_block() {
    let source = "- a\n   # b";
    assert_eq!(findings(source, "list-item-block-overindented"), Vec::new());
    let heading = findings(source, "block-opener-without-blank-line");
    assert_eq!(heading.len(), 1);
    assert!(heading[0].message.contains("a heading"));
}

#[test]
fn c2_b_081_definition_in_quote_reports_label_and_colon() {
    let definition = findings("> [a]: /u", "definition-in-container");
    assert_eq!(definition.len(), 1);
    let pos = definition[0].pos.expect("range");
    assert_eq!((pos.start_offset, pos.end_offset), (2, 6));
}

#[test]
fn c2_b_028_structural_openers_report_indent_in_each_owner() {
    for (source, offset, owner) in [
        (" > a", 1, "document"),
        (">  # h", 3, "a quote"),
        ("::: note\n  # h\n:::", 11, "a div"),
    ] {
        let findings = findings(source, "block-opener-overindented");
        assert_eq!(findings.len(), 1, "{source:?}: {findings:?}");
        assert_eq!(findings[0].pos.expect("range").start_offset, offset);
        assert!(findings[0].message.contains(owner));
    }
}

#[test]
fn c2_b_119_refused_div_marker_reports_inside_containers() {
    for source in ["> :::note", "- :::note"] {
        let markers = findings(source, "block-marker-as-text");
        assert_eq!(markers.len(), 1, "{source:?}: {markers:?}");
        assert_eq!(markers[0].pos.expect("range").start_offset, 2);
        assert!(markers[0].message.contains("a div fence"));
    }
}

#[test]
fn c2_h_172_math_bidi_control_is_reported() {
    let bidi = findings("$`x\u{2066}y`\n", "bidi-control-in-source");
    assert_eq!(bidi.len(), 1);
    let pos = bidi[0].pos.expect("range");
    assert_eq!(
        (pos.start_line, pos.start_column, pos.end_column),
        (1, 4, 5)
    );
    assert!(bidi[0].message.contains("U+2066"));
}

#[test]
fn c2_w_138_closing_marker_before_attributes_uses_span_variant() {
    let attribute = findings("*a * b*{.c}", "emphasis-marker-before-attributes");
    assert_eq!(attribute.len(), 1);
    let pos = attribute[0].pos.expect("range");
    assert_eq!((pos.start_offset, pos.end_offset), (6, 7));
    assert!(attribute[0].message.contains("closes the strong span"));
}

#[test]
fn c2_h_116_reserved_carrier_names_appear_in_loss_messages() {
    let rendered = carve2::render_with_diagnostics("[t]{data-carve-raw=html}", &Options::default());
    let loss = rendered
        .diagnostics
        .iter()
        .find(|finding| finding.code == "carrier-attribute-dropped")
        .expect("carrier loss");
    assert_eq!(
        loss.message,
        "Dropped the authored attribute data-carve-raw: the name is reserved for roundtrip import"
    );
    let pos = loss.pos.expect("range");
    assert_eq!((pos.start_offset, pos.end_offset), (4, 23));
}

#[test]
fn c2_h_054_raw_format_loss_uses_catalog_message() {
    let rendered = carve2::render_with_diagnostics("before\n\n```=latex\nx\n", &Options::default());
    let loss = rendered
        .diagnostics
        .iter()
        .find(|finding| finding.code == "raw-format-dropped")
        .expect("raw loss");
    assert_eq!(
        loss.message,
        "Dropped block raw format \"latex\" while rendering HTML"
    );
}

#[test]
fn c2_h_054_multiple_raw_losses_keep_each_kind_and_format() {
    let source = "`one`{=tex} and `two`{=latex}\n\n```=svg\nimage\n```\n";
    let rendered = carve2::render_with_diagnostics(source, &Options::default());
    let messages: Vec<_> = rendered
        .diagnostics
        .iter()
        .filter(|finding| finding.code == "raw-format-dropped")
        .map(|finding| finding.message.as_str())
        .collect();
    assert_eq!(
        messages,
        [
            "Dropped inline raw format \"tex\" while rendering HTML",
            "Dropped inline raw format \"latex\" while rendering HTML",
            "Dropped block raw format \"svg\" while rendering HTML",
        ]
    );
}

#[test]
fn c2_h_082_nested_image_alt_keeps_image_loss_variant() {
    let rendered = carve2::render_with_diagnostics("![a [b]](javascript:)", &Options::default());
    let loss = rendered
        .diagnostics
        .iter()
        .find(|finding| finding.code == "destination-denied")
        .expect("denied destination");
    assert_eq!(loss.message, "Blanked a denied image source");
}

#[test]
fn c2_h_075_semantic_attribute_range_is_the_key_and_value() {
    let attribute = findings("[x]{kbd=\"V\"}", "semantic-attribute-value-ignored");
    assert_eq!(attribute.len(), 1);
    let pos = attribute[0].pos.expect("range");
    assert_eq!((pos.start_offset, pos.end_offset), (4, 11));
}

#[test]
fn c2_h_082_denied_destination_uses_catalog_range_and_message() {
    let rendered = carve2::render_with_diagnostics("[](\u{202e}javascript:)", &Options::default());
    let loss = rendered
        .diagnostics
        .iter()
        .find(|finding| finding.code == "destination-denied")
        .expect("destination loss");
    assert_eq!(loss.message, "Blanked a denied destination scheme");
    let pos = loss.pos.expect("range");
    assert_eq!(
        (pos.start_line, pos.start_column, pos.end_column),
        (1, 4, 16)
    );
}

#[test]
fn c2_a_039_duplicate_id_reports_the_second_authored_id() {
    let source = "{#dup}\n# A\n\n{#dup}\n# B";
    let rendered = carve2::render_with_diagnostics(source, &Options::default());
    let duplicate = rendered
        .diagnostics
        .iter()
        .find(|finding| finding.code == "duplicate-id")
        .expect("duplicate id");
    let pos = duplicate.pos.expect("range");
    assert_eq!(
        (pos.start_line, pos.start_column, pos.end_column),
        (4, 2, 6)
    );
    assert!(duplicate.message.contains("Duplicate authored id \"dup\""));
}

#[test]
fn c2_w_138_table_marker_message_has_concrete_prefix() {
    let markers = findings("|{.a}< x |", "table-marker-run-padding");
    assert_eq!(markers.len(), 1);
    assert!(markers[0].message.contains("{< .a}"));
    let pos = markers[0].pos.expect("range");
    assert_eq!((pos.start_offset, pos.end_offset), (1, 6));
}
