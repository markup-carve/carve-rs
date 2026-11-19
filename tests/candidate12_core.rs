use carve::options::Options;
use carve::reader::{TreeError, read_tree_value};
use serde_json::{Value, json};

fn fixture(name: &str, id: &str) -> Value {
    let path = format!("{}/spec-trees/{name}.json", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(path).unwrap();
    let mut parser = serde_json::Deserializer::from_str(&text);
    parser.disable_recursion_limit();
    let value = serde::Deserialize::deserialize(&mut parser).unwrap_or(Value::Null);
    value["trees"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == id)
        .unwrap()["tree"]
        .clone()
}

#[test]
fn c2_h_152_table_width_keeps_significant_decimal_digits() {
    for (input, width) in [
        ("{widths=30.000000000001}\n| a |", "30.000000000001%"),
        ("{widths=0.000000000001}\n| a |", "0.000000000001%"),
    ] {
        let html = carve::render(input, &Options::default());
        assert!(html.contains(width), "{html}");
        assert!(!html.contains("e-"), "{html}");
    }
}

#[test]
fn c2_b_166_row_attribute_quoted_pipe_does_not_move_closer() {
    let html = carve::render("| a | b |{title=\"p|q\" .c}", &Options::default());
    assert!(html.contains("<tr class=\"c\" title=\"p|q\">"), "{html}");
    assert!(html.contains("<td>a</td><td>b</td>"), "{html}");
}

#[test]
fn c2_b_176_continuation_adds_header_cell() {
    let html = carve::render("|= Name |\n+ | Role |\n| Ada | Lead |", &Options::default());
    assert!(html.contains("<th scope=\"col\">Role</th>"), "{html}");
}

#[test]
fn c2_b_230_body_rows_partition_and_row_head_columns() {
    let tree: Value = serde_json::from_str(&carve::tree_json(
        "{body-rows=1,1 body-header-cols=,1}\n| a | b |\n| c | d |",
        &Options::default(),
    ))
    .unwrap();
    assert_eq!(tree["children"][0]["children"][0]["type"], "table_body");
    assert_eq!(tree["children"][0]["children"][1]["rowHeadColumns"], 1);
    let html = carve::render(
        "{body-rows=1,1 body-header-cols=,1}\n| a | b |\n| c | d |",
        &Options::default(),
    );
    assert!(html.contains("<th scope=\"row\">c</th>"), "{html}");
}

#[test]
fn c2_h_096_panel_cross_reference_uses_group_number_and_letter() {
    let html = carve::render(
        "{#g}\n::: figure\n{#a}\n![a](a.png)\n^ A\n\n{#b}\n![b](b.png)\n^ B\n:::\n^ Figure #: Both\n\nSee </#b>.",
        &Options::default(),
    );
    assert!(html.contains("<a href=\"#b\">Figure 1b</a>"), "{html}");
}

#[test]
fn c2_h_020_block_image_merges_paragraph_attrs_before_own() {
    let html = carve::render("{.outer k=one}\n![a](b){.inner k=two}", &Options::default());
    assert_eq!(
        html,
        "<img src=\"b\" alt=\"a\" class=\"outer inner\" k=\"two\">"
    );
}

#[test]
fn c2_h_147_ascii_heading_modes_preserve_or_lower_case() {
    for (name, id) in [
        ("ascii", "Getting-Started"),
        ("ascii-strict", "Getting-Started"),
        ("ascii-lower", "getting-started"),
        ("ascii-strict-lower", "getting-started"),
    ] {
        let opts = Options {
            heading_ids: carve::options::HeadingIds::from_name(name).unwrap(),
            ..Options::default()
        };
        let html = carve::render("# Getting Started", &opts);
        assert!(html.contains(&format!("id=\"{id}\"")), "{name}: {html}");
    }
}

#[test]
fn c2_b_182_cell_prefix_reads_both_alignment_marks() {
    let html = carve::render("|={?v .c #i} A |\n| a |", &Options::default());
    assert!(html.contains("id=\"i\""), "{html}");
    assert!(html.contains("vertical-align: bottom"), "{html}");
}

#[test]
fn c2_b_127_figure_group_demotes_nested_figure_div() {
    let tree: Value = serde_json::from_str(&carve::tree_json(
        "::: figure\n::: figure\n![a](a.png)\n^ (a) First\n:::\n^ Figure #: Both",
        &Options::default(),
    ))
    .unwrap();
    assert_eq!(tree["children"][0]["type"], "figure_group");
    assert_eq!(tree["children"][0]["children"][0]["type"], "div");
}

#[test]
fn c2_b_109_comment_closer_outside_item_column_keeps_list() {
    let tree: Value = serde_json::from_str(&carve::tree_json(
        "- a\n  %%%\n  hidden\n%%%\n- b\n\nafter",
        &Options::default(),
    ))
    .unwrap();
    assert_eq!(tree["children"][0]["type"], "list");
    assert_eq!(tree["children"][0]["children"].as_array().unwrap().len(), 2);
    assert_eq!(tree["children"][1]["type"], "paragraph");
}

#[test]
fn c2_b_137_region_marker_ends_leaf_and_blocks_caption() {
    let html = carve::render("a\n%% #region x\nb", &Options::default());
    assert_eq!(html, "<p>a</p>\n<p>b</p>");
    let html = carve::render("![a](i.png)\n%% #region x\n^ cap", &Options::default());
    assert!(html.contains("<p>^ cap</p>"), "{html}");
}

#[test]
fn c2_h_043_line_block_hard_break_merges_with_line_end() {
    let html = carve::render("::: |\na\\\nb\n:::", &Options::default());
    assert!(html.contains("<p>a<br>\nb</p>"), "{html}");
    assert!(!html.contains("<br>\n<br>"), "{html}");
}

#[test]
fn c2_h_015_line_block_align_writes_style() {
    let html = carve::render("{align=center}\n::: |\na\n:::", &Options::default());
    assert!(html.contains("style=\"text-align: center;\""), "{html}");
}

#[test]
fn c2_i_102_backslash_space_at_line_end_is_hard_break() {
    let html = carve::render("a\\ ", &Options::default());
    assert_eq!(html, "<p>a<br>\n</p>");
}

#[test]
fn c2_i_077_comment_inside_reference_label_takes_closer() {
    let tree: Value = serde_json::from_str(&carve::tree_json(
        "![a][b %% c]\n^ cap",
        &Options::default(),
    ))
    .unwrap();
    assert_eq!(tree["children"][0]["children"][1]["value"], "c]");
}

#[test]
fn c2_i_089_nested_note_openers_stay_text_or_form_links() {
    let tree: Value = serde_json::from_str(&carve::tree_json(
        "^[a [^n] ^[b] [^n](/u)]",
        &Options::default(),
    ))
    .unwrap();
    let children = tree["children"][0]["children"][0]["children"]
        .as_array()
        .unwrap();
    assert!(!children.iter().any(|n| n["type"] == "footnote_ref"));
    assert!(!children.iter().any(|n| n["type"] == "inline_footnote"));
    assert!(children.iter().any(|n| n["type"] == "link"));
}

#[test]
fn c2_b_083_link_definition_takes_title_then_one_attribute_block() {
    let html = carve::render("[x][a]\n\n[a]: /u 'T' {.c} \t", &Options::default());
    assert!(
        html.contains("href=\"/u\" title=\"T\" class=\"c\""),
        "{html}"
    );
}

#[test]
fn c2_h_163_comment_profile_denied_link_is_plain_text() {
    let opts = Options {
        profile: carve::options::Profile::Comment,
        ..Options::default()
    };
    let html = carve::render("[x *y*](javascript:a){.c}", &opts);
    assert_eq!(html, "<p>x y</p>");
}

#[test]
fn c2_a_091_table_cell_children_are_checked() {
    for id in ["table-cell-empty", "cell-paragraph-beside-block"] {
        assert!(
            matches!(
                read_tree_value(fixture("refused", id)),
                Err(TreeError::Reader { .. })
            ),
            "{id}"
        );
    }
    let mut mark = fixture("refused", "table-cell-empty");
    mark["children"][0]["children"][0]["children"][0]["children"][0]["span"] = json!("rowspan");
    assert!(read_tree_value(mark).is_ok());
}

#[test]
fn c2_h_013_span_semantic_case_and_ordinary_keys() {
    assert!(read_tree_value(fixture("examples-html", "html-80")).is_ok());
}

#[test]
fn c2_h_075_semantic_keys_match_only_lowercase_and_empty_time_has_no_datetime() {
    for (input, expected) in [
        ("[x]{KBD}", "<p><span KBD=\"\">x</span></p>"),
        ("[x]{time}", "<p><time>x</time></p>"),
        (
            "[x]{Time=12:00} and [y]{kbd Abbr=b}",
            "<p><span Time=\"12:00\">x</span> and <kbd Abbr=\"b\">y</kbd></p>",
        ),
    ] {
        assert_eq!(carve::render(input, &Options::default()), expected);
    }
}

#[test]
fn c2_a_203_annotation_ids_are_unique_across_tree() {
    assert!(read_tree_value(fixture("refused", "annotation-id-repeated-across-nodes")).is_err());
}

#[test]
fn c2_x_025_source_name_must_name_a_core_form() {
    assert!(matches!(
        read_tree_value(fixture("refused", "extension-source-name-raw-fence")),
        Err(TreeError::Reader { .. })
    ));
}

#[test]
fn c2_a_124_unqualified_extension_has_only_core_fields() {
    assert!(matches!(
        read_tree_value(fixture("refused", "unqualified-inline-extension-fallback")),
        Err(TreeError::Reader { .. })
    ));
}

#[test]
fn c2_a_098_smart_source_matches_kind() {
    assert!(matches!(
        read_tree_value(fixture("refused", "smart-punctuation-other-source")),
        Err(TreeError::Reader { .. })
    ));
}

#[test]
fn c2_a_056_ordered_start_is_bounded() {
    assert!(matches!(
        read_tree_value(fixture("refused", "marker-start-out-of-range")),
        Err(TreeError::Schema { .. })
    ));
}

#[test]
fn c2_a_123_qualified_extension_is_declared() {
    for id in [
        "extension-undeclared",
        "qualified-extension-without-declaration",
    ] {
        assert!(
            matches!(
                read_tree_value(fixture("refused", id)),
                Err(TreeError::Reader { .. })
            ),
            "{id}"
        );
    }
}

#[test]
fn c2_a_058_extension_empty_defaults_are_refused() {
    for id in ["default-empty-props", "default-empty-resource-ids"] {
        assert!(
            matches!(
                read_tree_value(fixture("refused", id)),
                Err(TreeError::Schema { .. })
            ),
            "{id}"
        );
    }
}

#[test]
fn c2_a_093_rowspan_stays_within_group() {
    assert!(matches!(
        read_tree_value(fixture("refused", "rowspan-past-group")),
        Err(TreeError::Reader { .. })
    ));
}

#[test]
fn c2_x_030_unqualified_extension_fallback_depth_is_bounded() {
    assert!(matches!(
        read_tree_value(fixture("refused", "unqualified-extension-depth-33")),
        Err(TreeError::Reader { .. })
    ));
}

#[test]
fn c2_b_042_ambiguous_first_ordered_marker_uses_next_marker() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/spec-conformance/clause-cases.json"
    );
    let source = std::fs::read_to_string(path).unwrap();
    let mut parser = serde_json::Deserializer::from_str(&source);
    parser.disable_recursion_limit();
    let cases: Value = serde::Deserialize::deserialize(&mut parser).unwrap();
    for number in 280..=294 {
        let id = format!("CC-{number}");
        let case = cases["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["id"] == id)
            .unwrap();
        if let Some(expected) = case["expected"]["html"].as_str() {
            assert_eq!(
                carve::render(case["input"].as_str().unwrap(), &Options::default()),
                expected,
                "{id}"
            );
        }
    }
}

#[test]
fn c2_b_137_comment_only_line_preserves_ordered_lookahead_blank_count() {
    let html = carve::render("v. a\n\n\n%% c\n\nvi. b\n", &Options::default());
    assert!(html.starts_with("<ol type=\"a\" start=\"22\">"), "{html}");
}

#[test]
fn c2_h_013_consumed_attribute_names_ignore_ascii_case() {
    let opts = Options::default();
    assert_eq!(
        carve::render("[x](/u){REL=javascript:x}", &opts),
        "<p><a href=\"/u\">x</a></p>"
    );
    let html = carve::render("{WIDTHS=not-a-width}\n| a |", &opts);
    assert!(html.starts_with("<table>"), "{html}");
}

#[test]
fn c2_h_167_rel_words_split_at_ascii_whitespace() {
    let html = carve::render("[x](/u){rel=\"\t\"}", &Options::default());
    assert_eq!(html, "<p><a href=\"/u\">x</a></p>");
}

#[test]
fn c2_h_015_align_is_folded_and_trimmed_before_style() {
    for input in ["{align=\" CENTER \"}\nText", "{align=\"\tRIGHT\t\"}\n= H"] {
        let html = carve::render(input, &Options::default());
        assert!(html.contains("text-align: "), "{html}");
        assert!(!html.contains("align="), "{html}");
    }
}

#[test]
fn c2_h_201_derived_abbreviation_and_xref_text_share_a_bounded_budget() {
    let expansion = "X".repeat(10_000);
    let source = format!("*[HT]: {expansion}\n\n{}", "HT ".repeat(150));
    let got = carve::render_with_diagnostics(&source, &Options::default());
    assert!(got.html.matches("<abbr").count() < 150);
    assert!(got.html.contains("HT HT"));
    assert_eq!(
        got.codes()
            .iter()
            .filter(|code| *code == "derived-text-budget")
            .count(),
        1
    );

    let heading = "H".repeat(10_000);
    let source = format!("{{#target}}\n= {heading}\n\n{}", "</#target> ".repeat(150));
    let got = carve::render_with_diagnostics(&source, &Options::default());
    assert!(got.html.len() < 1_100_000, "{}", got.html.len());
    assert!(got.html.contains("<a href=\"#target\">target</a>"));
    assert!(got.codes().contains(&"derived-text-budget".to_string()));
}

#[test]
fn c2_h_201_json_tree_source_length_cannot_raise_derived_budget() {
    let heading = "H".repeat(10_000);
    let source = format!("{{#target}}\n= {heading}\n\n{}", "</#target> ".repeat(150));
    let mut tree: Value =
        serde_json::from_str(&carve::tree_json(&source, &Options::default())).unwrap();
    tree["srcByteLength"] = serde_json::json!(1_000_000_000);
    let json = tree.to_string();
    let read = carve::reader::read_tree_json(&json).unwrap();
    assert_eq!(carve::reader::write_tree_json(&read), json);
    let html = carve::render_tree(read.document(), &Options::default()).unwrap();
    assert!(html.len() < 1_100_000, "{}", html.len());
}

#[test]
fn c2_a_171_reader_counts_list_description_and_inline_note_levels() {
    use carve::reader::read_tree_value;
    for kind in ["list", "definition_list", "inline_footnote"] {
        let mut child = serde_json::json!({"type":"text","value":"x"});
        let count = if kind == "inline_footnote" { 201 } else { 101 };
        for _ in 0..count {
            child = if kind == "inline_footnote" {
                serde_json::json!({"type":kind,"children":[child]})
            } else {
                serde_json::json!({"type":kind,"children":[{"type":"list_item","children":[child]}]})
            };
        }
        let tree = serde_json::json!({"type":"document","astVersion":"2.0","children":[child]});
        assert!(read_tree_value(tree).is_err(), "{kind}");
    }
}

#[test]
fn c2_h_176_lone_surrogate_in_supplied_name_is_rejected_without_panic() {
    let json = concat!(
        r#"{"type":"document","astVersion":"2.0","children":[{"type":"paragraph","children":["#,
        r#"{"type":"mention","kind":"user","target":"\ud800","label":"ok"}]}]}"#,
    );
    assert!(carve::reader::read_tree_json(json).is_err());
}

#[test]
fn c2_i_074_image_alt_removes_only_punctuation_escapes() {
    for (input, alt) in [
        (r"![5 \* 3 \[draft\]](/i.png)", "alt=\"5 * 3 [draft]\""),
        (r"![a\q](/i.png)", "alt=\"a\\q\""),
        (r"![a\ b](/i.png)", "alt=\"a\\ b\""),
    ] {
        let html = carve::render(input, &Options::default());
        assert!(html.contains(alt), "{html}");
    }
}

#[test]
fn c2_b_197_block_image_uses_inline_comment_closer_rule() {
    // D2 (18 November): `%%` directly after a braced opener is text, so the
    // first `]` closes the bracket and no image forms (C2-I-023).
    let html = carve::render("![{*%%]*}]()\n", &Options::default());
    assert_eq!(html, "<p>![<strong>%%]</strong>]()</p>");
}

#[test]
fn c2_b_171_header_mark_needs_a_boundary_after_equals() {
    for (source, fragment) in [
        ("|=a|\n| x |", "<td>=a</td>"),
        ("|==a|\n| x |", "<td>==a</td>"),
        ("|=SUM(A1)|", "<td>=SUM(A1)</td>"),
        ("|= a|", "<th scope=\"col\">a</th>"),
    ] {
        let html = carve::render(source, &Options::default());
        assert!(html.contains(fragment), "{html}");
    }
}

#[test]
fn c2_b_164_frontmatter_format_is_ascii_alphanumeric_with_case_preserved() {
    for (source, format) in [("---1\nx\n---", "1"), ("--- Yaml\nx\n---", "Yaml")] {
        let tree: Value =
            serde_json::from_str(&carve::tree_json(source, &Options::default())).unwrap();
        assert_eq!(tree["children"][0]["format"], format);
    }
    let tree: Value = serde_json::from_str(&carve::tree_json(
        "---x-y\na: b\n---\nafter",
        &Options::default(),
    ))
    .unwrap();
    assert_ne!(tree["children"][0]["type"], "frontmatter");
}

#[test]
fn c2_i_097_cross_reference_ids_match_after_nfc() {
    let html = carve::render("# Café\n\nSee </#Cafe\u{301}>.", &Options::default());
    assert!(html.contains("<a href=\"#Café\">Café</a>"), "{html}");
}

#[test]
fn c2_i_084_heading_lookup_uses_nfc_and_skips_empty_key() {
    let html = carve::render("# Café\n\n[Cafe\u{301}][]", &Options::default());
    assert!(html.contains("<a href=\"#Café\">Cafe\u{301}</a>"), "{html}");
    let html = carve::render("> # :a:\n>\n> # b\n>\n> [:a:][]", &Options::default());
    assert!(!html.contains("<a href"), "{html}");
}
