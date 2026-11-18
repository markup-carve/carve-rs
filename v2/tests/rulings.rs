use carve2::{Options, render};

fn tree(src: &str) -> serde_json::Value {
    let arena = carve2::Arena::new();
    let document = carve2::parse_resolved_in(src, &Options::default(), &arena).unwrap();
    serde_json::from_str(&carve2::to_json(&document)).unwrap()
}

#[test]
fn c2_b_105_admonition_glued_attrs_are_reported_and_dropped() {
    assert_eq!(
        render("::: note{.x}\nn\n:::", &Options::default()),
        "<aside class=\"admonition note\" aria-label=\"Note\">\n  <p>n</p>\n</aside>"
    );
}

#[test]
fn c2_b_119_title_and_label_need_space_behind_div_run() {
    assert_eq!(render(":::[x] y", &Options::default()), "<p>:::[x] y</p>");
    assert!(render("::: [x]\ny\n:::", &Options::default()).contains("class=\"div-label\""));
}

#[test]
fn c2_b_119_title_and_label_need_space_after_name() {
    let glued = tree("::: v\"\"\n:::");
    assert!(glued["children"][0].get("title").is_none());
    let spaced = tree("::: v \"\"\n:::");
    assert!(spaced["children"][0].get("title").is_some());
}

#[test]
fn c2_b_116_raw_format_needs_initial_ascii_letter() {
    let invalid = tree("```=/\n``` ");
    assert_eq!(invalid["children"][0]["type"], "code_block");
    let valid = tree("```=Html_2\n```");
    assert_eq!(valid["children"][0]["type"], "raw_block");
}

#[test]
fn c2_b_117_fence_metadata_can_follow_the_run_without_a_language() {
    assert_eq!(tree("```[]")["children"][0]["groupLabel"], "");
    assert!(tree("```\"T\"")["children"][0].get("title").is_some());
    assert!(tree("```py[]")["children"][0].get("groupLabel").is_none());
}

#[test]
fn c2_b_108_comment_fence_keeps_trailing_space_on_its_opener() {
    assert_eq!(tree("%%%> ")["children"][0]["value"], "> ");
    assert_eq!(tree("%%%  a\t ")["children"][0]["value"], "a\t ");
}

#[test]
fn c2_a_068_last_hardbreaks_class_forces_hard_breaks() {
    let html = render("{.hardbreaks}\n:::\na\nb\n:::", &Options::default());
    assert!(html.contains("a<br>\n"), "{html}");
}

#[test]
fn c2_b_125_only_a_bare_colon_run_closes_a_div() {
    let html = render(":::\n::: %", &Options::default());
    assert_eq!(html, "<div>\n  <div>\n\n  </div>\n</div>");
    assert_eq!(
        tree(":::\n::: :")["children"][0]["children"][0]["type"],
        "div"
    );
}

#[test]
fn c2_b_033_tab_after_required_list_space_is_content_indent() {
    assert_eq!(
        render("- \ta", &Options::default()),
        "<ul>\n  <li>a</li>\n</ul>"
    );
    assert_eq!(tree("- \ta")["children"][0]["type"], "list");
}

#[test]
fn c2_b_046_task_box_must_follow_only_the_required_spaces() {
    assert_eq!(
        render("-  [ ] q", &Options::default()),
        "<ul>\n  <li>[ ] q</li>\n</ul>"
    );
    assert_eq!(
        tree("-  [ ] q")["children"][0]["children"][0]["task"],
        serde_json::Value::Null
    );
}

#[test]
fn c2_a_073_code_title_is_one_literal_text_node_even_when_empty() {
    let t = tree("```python \"\"\ncode\n```");
    assert_eq!(t["children"][0]["title"].as_array().unwrap().len(), 1);
    assert_eq!(t["children"][0]["title"][0]["value"], "");

    let t = tree("```python \"src/*a* \\!\"\ncode\n```");
    assert_eq!(t["children"][0]["title"].as_array().unwrap().len(), 1);
    assert_eq!(t["children"][0]["title"][0]["value"], "src/*a* !");
}

#[test]
fn c2_h_016_table_panel_carries_source_line() {
    let opts = Options {
        source_lines: true,
        ..Options::default()
    };
    let html = render(
        ":::: figure\n![a](a.png)\n^ First\n\n| t |\n^ Second\n::::",
        &opts,
    );
    assert!(
        html.contains("<figure class=\"carve-figure-panel\" data-source-line=\"5\">"),
        "{html}"
    );
}

#[test]
fn c2_b_197_invalid_image_tail_is_not_a_block_image() {
    let t = tree("![i](a.png\t)\n^ cap");
    assert_eq!(t["children"][0]["type"], "paragraph");
    assert!(t["children"][0].get("role").is_none());
    assert_eq!(t["children"][0]["children"][1]["type"], "soft_break");
}

#[test]
fn c2_i_040_empty_block_after_a_cell_attribute_is_text() {
    assert!(render("|{c}{}|", &Options::default()).contains("<td c=\"\">{}</td>"));
}

#[test]
fn c2_b_205_shift_has_at_most_nine_digits() {
    assert_eq!(
        render(
            "{{ a.crv @shift:-123456789 }}\n\n{{ a.crv @shift:1234567890 }}",
            &Options::default()
        ),
        "<p>{{ a.crv @shift:-123456789 }}</p>\n<p>{{ a.crv @shift:1234567890 }}</p>"
    );
}

#[test]
fn c2_i_117_closing_mark_is_not_a_plus_minus_neighbor() {
    assert_eq!(
        render("{++-+} and {++-1+} and a{%x%}+-b", &Options::default()),
        "<p><ins>±</ins> and <ins>±1</ins> and a±b</p>"
    );
    assert_eq!(render("+-{%x%}-", &Options::default()), "<p>+--</p>");
}

#[test]
fn c2_b_211_invalid_column_lists_have_no_effect_and_stay_in_attrs() {
    let src = "{aligns=right,x widths=25,x}\n| a | b |";
    let t = tree(src);
    let table = &t["children"][0];
    assert_eq!(
        table["attrs"]["keyValues"],
        serde_json::json!([["aligns", "right,x"], ["widths", "25,x"]])
    );
    assert!(
        table["columns"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c.get("align").is_none() && c.get("width").is_none())
    );
    assert!(!render(src, &Options::default()).contains("<colgroup>"));
}

#[test]
fn c2_b_227_invalid_and_inapplicable_counts_stay_in_attrs() {
    let src = "{header-rows=2 footer-rows=x}\n| a | b |\n| c | d |";
    let t = tree(src);
    assert_eq!(
        t["children"][0]["attrs"]["keyValues"],
        serde_json::json!([["header-rows", "2"], ["footer-rows", "x"]])
    );
    let html = render(src, &Options::default());
    assert!(!html.contains("header-rows=") && !html.contains("footer-rows="));
}

#[test]
fn c2_b_211_short_valid_list_applies_and_invalid_tail_cancels_all() {
    let valid = tree("{aligns=right widths=25}\n| a | b |");
    let columns = &valid["children"][0]["columns"];
    assert_eq!(columns[0]["align"], "right");
    assert_eq!(columns[0]["width"], 0.25);
    assert!(columns[1].get("align").is_none());
    assert!(valid["children"][0].get("attrs").is_none());

    let invalid = tree("{aligns=right,left,x widths=25,50,0}\n| a | b |");
    let columns = invalid["children"][0]["columns"].as_array().unwrap();
    assert!(
        columns
            .iter()
            .all(|c| c.get("align").is_none() && c.get("width").is_none())
    );
}

#[test]
fn c2_i_084_heading_lookup_reads_markup_in_the_key() {
    let src = "# A bold title\n\n[A *bold* title][] and [x][A {*bold*} title]";
    let html = render(src, &Options::default());
    assert!(html.contains("<a href=\"#A-bold-title\">A <strong>bold</strong> title</a>"));
    assert!(html.contains("<a href=\"#A-bold-title\">x</a>"));
    let t = tree(src);
    assert!(t.to_string().contains("#A-bold-title"));
}

#[test]
fn c2_h_020_attributed_block_image_merges_paragraph_attrs() {
    let src = "{.c #i}\n![a](x.png)\n\n{:de}\n![b](y.png)\n\n![c](z.png)";
    assert_eq!(
        render(src, &Options::default()),
        concat!(
            "<img src=\"x.png\" alt=\"a\" id=\"i\" class=\"c\">\n",
            "<img src=\"y.png\" alt=\"b\" lang=\"de\">\n",
            "<img src=\"z.png\" alt=\"c\">"
        )
    );
}

#[test]
fn c2_h_064_table_panel_has_figure_wrapper() {
    let src = ":::: figure\n![a](a.png)\n^ First\n\n| t |\n^ Second\n::::";
    let html = render(src, &Options::default());
    assert!(html.contains("  <figure class=\"carve-figure-panel\">\n    <table>"));
    assert!(html.contains("      <caption>Second</caption>"));
    assert!(html.contains("    </table>\n  </figure>"));
}

#[test]
fn c2_h_165_degraded_table_keeps_caption_number() {
    let mut options = Options::default();
    options.profile = carve2::options::Profile::Comment;
    let src = "| t |\n^ Table #: tc";
    assert_eq!(render(src, &options), "<p>t</p>\n<p>Table 1: tc</p>");
}

#[test]
fn c2_h_177_references_to_degraded_heading_render_unresolved() {
    let mut options = Options::default();
    options.profile = carve2::options::Profile::Comment;
    let src = "## Sub *em* code\n\n[Sub em code][] and </#Sub-em-code>";
    let html = render(src, &options);
    assert_eq!(
        html,
        "<p>## Sub em code</p>\n<p>[Sub em code][] and Sub em code</p>"
    );
    assert!(tree(src).to_string().contains("#Sub-em-code"));
}

#[test]
fn c2_h_165_whole_block_degradation_uses_code_and_math_values() {
    let mut options = Options::default();
    options.profile = carve2::options::Profile::Minimal;
    let src = "![i](a.png)\n^ a `x` b $`m`\n\n> q `c` $`n`";
    assert_eq!(
        render(src, &options),
        "<p>[img: i]</p>\n<p>a x b m</p>\n<p>&gt; q c n</p>"
    );
}

#[test]
fn c2_h_126_social_template_respects_profile_and_encoding() {
    let mut options = Options::default();
    options.social = Some(carve2::options::Social {
        mention_url: Some("https://x.org/u/{name}".into()),
        ..Default::default()
    });
    assert_eq!(
        render("@ann.b and #topic", &options),
        concat!(
            "<p><a class=\"mention\" href=\"https://x.org/u/ann.b\">@ann.b</a>",
            " and <span class=\"tag\"><strong>#topic</strong></span></p>"
        )
    );
    options.profile = carve2::options::Profile::Minimal;
    assert_eq!(
        render("@ann", &options),
        "<p><span class=\"mention\"><strong>@ann</strong></span></p>"
    );
}

#[test]
fn c2_i_118_open_delimiter_counts_at_link_and_image_cap() {
    let at_cap = format!("{}*[x](u) ![y](v){}", "{^".repeat(199), "^}".repeat(199));
    let html = render(&at_cap, &Options::default());
    assert!(html.contains("*[x](u) ![y](v)"));
    assert!(!html.contains("<a href=") && !html.contains("<img "));

    let below = format!("{}*[x](u) ![y](v){}", "{^".repeat(198), "^}".repeat(198));
    let html = render(&below, &Options::default());
    assert!(html.contains("*<a href=\"u\">x</a> <img src=\"v\" alt=\"y\">"));
}

#[test]
fn c2_i_118_capped_bracket_opener_never_takes_a_closer() {
    let src = format!("{}:n[[]{}", "[".repeat(199), "]".repeat(199));
    let html = render(&src, &Options::default());
    assert!(html.contains("<span class=\"ext-n\">[</span>"));
    assert!(!html.contains("<span class=\"ext-n\">[]</span>"));
}

#[test]
fn c2_h_178_roundtrip_abbreviation_runs_keep_every_definition() {
    let options = Options {
        roundtrip: true,
        ..Options::default()
    };
    let src = "*[A]: First\n*[A]: Second\n*[U]: Unused\n\nA.";
    let html = render(src, &options);
    assert!(html.starts_with(
        "<template data-carve-abbreviations>*[A]: First\n\n*[A]: Second\n\n*[U]: Unused</template>"
    ));
    assert!(html.contains("<abbr title=\"First\">A</abbr>"));
}

#[test]
fn c2_h_179_abbreviation_template_writes_attrs_and_escapes_text() {
    let options = Options {
        roundtrip: true,
        ..Options::default()
    };
    let html = render("{#x .c}\n*[A]: R&D <research> \"and\"\n\nA.", &options);
    assert!(html.starts_with(
        "<template data-carve-abbreviations>{#x .c}\n*[A]: R&amp;D &lt;research&gt; \"and\"</template>"
    ));
}

#[test]
fn c2_a_158_caption_mark_and_empty_cells_have_positions() {
    let t = tree("![i](a.png)\n^ Figure #: cap");
    let children = &t["children"][0]["caption"]["children"][0]["children"];
    assert_eq!(children[0]["pos"]["startOffset"], 14);
    assert_eq!(children[1]["pos"]["startOffset"], 21);
    assert_eq!(children[1]["pos"]["endOffset"], 22);
    assert_eq!(children[2]["pos"]["startOffset"], 22);

    let t = tree("| a | |\n| b | < |");
    let rows = &t["children"][0]["children"][0]["children"];
    let empty = &rows[0]["children"][1];
    assert_eq!(empty["pos"]["startOffset"], 5);
    assert_eq!(empty["pos"]["endOffset"], 6);
    assert!(empty["children"][0].get("pos").is_none());
    // A cell that holds a span mark spans the text between its pipes.
    let span = &rows[1]["children"][1];
    assert_eq!(span["pos"]["startOffset"], 13);
    assert_eq!(span["pos"]["endOffset"], 16);
}

#[test]
fn c2_a_158_joined_cell_keeps_positions_within_one_piece() {
    let t = tree("| *a* | b |\n+ c | d |");
    let cell = &t["children"][0]["children"][0]["children"][0]["children"][0];
    assert!(cell.get("pos").is_none());
    let para = &cell["children"][0];
    assert!(para.get("pos").is_none());
    assert_eq!(para["children"][0]["pos"]["startOffset"], 2);
    assert_eq!(para["children"][0]["children"][0]["pos"]["startOffset"], 3);
    assert!(para["children"][1].get("pos").is_none());
}

#[test]
fn c2_a_151_table_group_owns_continuation_line() {
    let t = tree("| 1 | 2 |\n+ a | b | c |");
    let body = &t["children"][0]["children"][0];
    assert_eq!(body["pos"]["endOffset"], 23);

    let t = tree("| *a* | b |\n+ c | d |");
    let body = &t["children"][0]["children"][0];
    assert_eq!(body["pos"]["endOffset"], 21);
}

#[test]
fn c2_b_184_continuation_under_row_span_joins_origin_cell() {
    let src = "| a | b |\n| ^ | c |\n+ x | d |";
    let html = render(src, &Options::default());
    assert!(html.contains(">a x</td>"), "{html}");
    let t = tree(src);
    let rows = &t["children"][0]["children"][0]["children"];
    let origin = &rows[0]["children"][0];
    assert_eq!(origin["children"][0]["children"][0]["value"], "a x");
    assert!(origin.get("pos").is_none());
    assert_eq!(rows[1]["children"][0]["span"], "rowspan");
    // C2-A-158: the mark cell takes no piece of the continuation and spans its pipes.
    assert_eq!(rows[1]["children"][0]["pos"]["startOffset"], 11);
}

#[test]
fn c2_a_086_comment_before_continuation_stays_in_group() {
    let t = tree("| a |\n%% x\n+ b |");
    let table = &t["children"][0];
    assert_eq!(table["children"].as_array().unwrap().len(), 1);
    assert_eq!(table["children"][0]["children"][1]["type"], "comment");
    assert_eq!(table["children"][0]["children"][1]["value"], "x");
}

#[test]
fn c2_a_151_final_line_comment_owns_dropped_spaces() {
    let t = tree("c %% z  ");
    let p = &t["children"][0];
    assert_eq!(p["pos"]["endOffset"], 8);
    assert_eq!(p["children"][1]["pos"]["startOffset"], 1);
    assert_eq!(p["children"][1]["pos"]["endOffset"], 8);

    let t = tree("a %% x  \nb");
    let nodes = &t["children"][0]["children"];
    assert_eq!(nodes[1]["pos"]["startOffset"], 1);
    assert_eq!(nodes[1]["pos"]["endOffset"], 8);
    assert_eq!(nodes[2]["pos"]["startOffset"], 8);
    assert_eq!(nodes[2]["pos"]["endOffset"], 9);

    let t = tree("{{ a.crv }}  \n");
    assert_eq!(t["children"][0]["pos"]["endOffset"], 13);

    let t = tree("a  \nb");
    assert_eq!(t["children"][0]["children"][1]["pos"]["startOffset"], 1);
    assert_eq!(t["children"][0]["children"][1]["pos"]["endOffset"], 4);
}

#[test]
fn c2_a_086_trailing_table_row_comments_are_siblings() {
    let t = tree("| 1 | 2 | %% note\n| 3 | 4 |");
    let body = &t["children"][0]["children"][0];
    assert_eq!(body["children"][0]["type"], "table_row");
    assert_eq!(body["children"][0]["pos"]["endOffset"], 9);
    assert_eq!(body["children"][1]["type"], "comment");
    assert_eq!(body["children"][1]["value"], "note");
    assert_eq!(body["children"][1]["pos"]["startOffset"], 9);
    assert_eq!(body["children"][1]["pos"]["endOffset"], 17);
    assert_eq!(body["children"][2]["type"], "table_row");

    let t = tree("| 1 | 2 |{.r} %% note\n| 3 | 4 | %% n2");
    let table = &t["children"][0];
    assert_eq!(
        table["children"][0]["children"][0]["attrs"]["classes"][0],
        "r"
    );
    assert_eq!(table["children"][0]["children"][1]["value"], "note");
    assert_eq!(table["children"][1]["value"], "n2");

    assert_eq!(
        tree("| a | b | %% note | x")["children"][0]["type"],
        "paragraph"
    );
    assert_eq!(tree("||%%")["children"][0]["type"], "paragraph");
}

#[test]
fn c2_b_218_byte_source_replaces_maximal_subparts_with_original_offsets() {
    let opts = Options::default();
    for input in [&b"a\xffb"[..], &b"a\xe2\x82b"[..]] {
        let arena = carve2::Arena::new();
        let doc = carve2::try_parse_resolved_bytes_in(input, &opts, &arena).unwrap();
        let t: serde_json::Value = serde_json::from_str(&carve2::to_json(&doc)).unwrap();
        assert_eq!(t["srcByteLength"], input.len());
        assert_eq!(t["children"][0]["children"][0]["value"], "a�b");
        assert_eq!(t["children"][0]["pos"]["endOffset"], input.len());
        assert_eq!(
            carve2::try_render_bytes(input, &opts).unwrap(),
            "<p>a�b</p>"
        );
        assert_eq!(
            t["children"][0]["children"][0]["pos"]["endOffset"],
            input.len()
        );
    }
}

#[test]
fn c2_b_226_delimited_records_keep_quotes_spaces_and_empty_rows() {
    let t = tree("{table}\n```csv\na,\"b \"\"q\"\"\"x, d\n*x*,\n\n\"m\nn\"x,y\n\n```");
    let groups = &t["children"][0]["children"];
    let rows: Vec<_> = groups
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|g| g["children"].as_array().unwrap())
        .collect();
    let value = |row: usize, cell: usize| {
        rows[row]["children"][cell]["children"][0]["children"][0]["value"].clone()
    };
    assert_eq!(value(0, 0), "a");
    assert_eq!(value(0, 1), "b \"q\"x");
    assert_eq!(value(0, 2), " d");
    assert_eq!(value(1, 0), "*x*");
    assert_eq!(
        rows[1]["children"][1]["children"][0]["children"],
        serde_json::json!([])
    );
    assert_eq!(
        rows[2]["children"][0]["children"][0]["children"],
        serde_json::json!([])
    );
    assert_eq!(value(3, 0), "m\nnx");
    assert_eq!(rows.len(), 4);
    assert_eq!(
        tree("{table}\n```CSV\na,b\n```")["children"][0]["type"],
        "code_block"
    );
    assert_eq!(
        tree("{table}\n```tsv\na\tb,c\n```")["children"][0]["type"],
        "table"
    );
}

#[test]
fn c2_b_130_line_block_keeps_lead_after_container_strip() {
    let t = tree(" ::: |\n \n x\n :::");
    let lines = &t["children"][0]["children"];
    assert_eq!(lines[0]["type"], "line");
    // C2-A-158: a line with no children has no `pos`.
    assert!(lines[0].get("pos").is_none());
    assert_eq!(lines[0]["children"], serde_json::json!([]));
    assert_eq!(lines[1]["children"][0]["value"], " x");

    let t = tree("- > a\n  > \t::: |\n  > \tx\n  > \t:::");
    let line = &t["children"][0]["children"][0]["children"][0]["children"][1]["children"][0];
    assert_eq!(line["children"][0]["value"], "\tx");
}

#[test]
fn c2_b_029_fence_body_uses_opener_indent_left_of_item_content() {
    let src = " - [\n  ```!\n    a";
    assert!(render(src, &Options::default()).contains("<code>  a\n</code>"));
    let quoted = src
        .lines()
        .map(|line| format!("> {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(render(&quoted, &Options::default()).contains("<code>  a\n</code>"));
}

#[test]
fn c2_i_002_line_end_spaces_are_removed_before_inline_forms() {
    for (src, kind) in [
        ("`a  \n  b`", "code"),
        ("!`a  \n  b`", "literal_inline"),
        ("$`a  \n  b`", "math"),
        ("`a  \n  b`{=html}", "raw_inline"),
        ("{%a  \n  b%}", "comment"),
    ] {
        let t = tree(src);
        let node = &t["children"][0]["children"][0];
        assert_eq!(node["type"], kind, "{src}");
        assert_eq!(node["value"], "a\nb", "{src}");
    }
}

#[test]
fn c2_h_032_task_label_reads_all_direct_paragraphs() {
    let html = render(
        "- [ ] first *a*\n\n  second\n\n  > quote",
        &Options::default(),
    );
    assert!(html.contains("aria-label=\"first a second\""));
    assert!(!html.contains("aria-label=\"first a second quote\""));
}

#[test]
fn c2_b_040_uppercase_roman_marker_needs_two_spaces() {
    assert_eq!(tree("II. Background")["children"][0]["type"], "paragraph");
    assert_eq!(tree("II.  Background")["children"][0]["type"], "list");
}

#[test]
fn c2_a_109_heading_ids_are_read_by_resolution() {
    let arena = carve2::Arena::new();
    let off = Options {
        heading_ids: carve2::options::HeadingIds::Off,
        ..Options::default()
    };
    let doc = carve2::parse_resolved_in("# Mixed Name", &off, &arena).unwrap();
    let t: serde_json::Value = serde_json::from_str(&carve2::to_json(&doc)).unwrap();
    assert!(t["children"][0].get("generatedId").is_none());

    let lower = Options {
        heading_ids: carve2::options::HeadingIds::Lower,
        ..Options::default()
    };
    let doc = carve2::parse_resolved_in("# Mixed Name", &lower, &arena).unwrap();
    let t: serde_json::Value = serde_json::from_str(&carve2::to_json(&doc)).unwrap();
    assert_eq!(t["children"][0]["generatedId"], "mixed-name");
}

#[test]
fn c2_b_122_two_spaces_leave_fence_without_language() {
    let t = tree("```  b\ntext\n```");
    assert_eq!(t["children"][0]["type"], "code_block");
    assert!(t["children"][0].get("syntax").is_none());
    assert_eq!(
        render("```  b\ntext\n```", &Options::default()),
        "<pre><code>text\n</code></pre>"
    );
}

#[test]
fn c2_h_069_comment_only_line_preserves_neighboring_spaces() {
    let html = render("^[ {%\n%}\n]", &Options::default());
    assert!(html.contains("<p> <a href=\"#fnref1\""));
}

#[test]
fn c2_b_125_bare_run_closes_innermost_div_it_reaches() {
    let html = render("::: spoiler\nx\n:::: spoiler\nx\n:::", &Options::default());
    assert_eq!(
        html,
        "<div class=\"spoiler\">\n  <p>x</p>\n  <div class=\"spoiler\">\n    <p>x</p>\n  </div>\n</div>"
    );
}

#[test]
fn c2_b_109_bare_div_closer_stays_in_its_container() {
    let src = "::: a\n- x\n  :::\ny";
    let t = tree(src);
    assert_eq!(t["children"].as_array().unwrap().len(), 1);
    assert_eq!(t["children"][0]["type"], "div");
    assert_eq!(t["children"][0]["children"].as_array().unwrap().len(), 2);
    assert_eq!(t["children"][0]["children"][0]["type"], "list");
    assert_eq!(t["children"][0]["children"][1]["type"], "paragraph");
}

#[test]
fn c2_b_197_block_image_tail_keeps_multibyte_destination() {
    for src in ["![i](a\\é.png)\n^ cap", "![i](a\u{a0}b.png)\n^ cap"] {
        let t = tree(src);
        assert_eq!(t["children"][0]["type"], "paragraph", "{src}");
        assert!(t["children"][0].get("caption").is_some(), "{src}");
    }
    assert_eq!(
        tree("![i](a.png\t)\n^ cap")["children"][0]["type"],
        "paragraph"
    );
}

#[test]
fn c2_a_156_unescaped_code_title_has_no_source_slice_position() {
    let changed = tree("```python \"src/*a* \\!\"\ncode\n```");
    let title = &changed["children"][0]["title"][0];
    assert_eq!(title["value"], "src/*a* !");
    assert!(title.get("pos").is_none());

    let literal = tree("```python \"src/app.py\"\ncode\n```");
    assert!(literal["children"][0]["title"][0].get("pos").is_some());
}

#[test]
fn c2_i_033_code_title_reads_escaped_quote() {
    let src = "```py \"a \\\" d\"\ncode\n```";
    let title = &tree(src)["children"][0]["title"][0];
    assert_eq!(title["value"], "a \" d");
    assert!(title.get("pos").is_none());
    assert!(render(src, &Options::default()).contains("title=\"a &quot; d\""));
}

#[test]
fn c2_i_013_destination_backslash_does_not_hide_whitespace() {
    for src in [
        "![i](a\\ b.png)\n^ cap",
        "![i](a\\\tb.png)\n^ cap",
        "![a](\\ )\n^ cap",
    ] {
        assert_eq!(tree(src)["children"][0]["type"], "paragraph", "{src}");
    }
}

#[test]
fn c2_i_068_image_title_needs_exactly_one_space() {
    assert_eq!(
        tree("![i](a.png  \"t\")\n^ cap")["children"][0]["type"],
        "paragraph"
    );
    assert_eq!(
        tree("![i](a \"t\"\t)\n^ cap")["children"][0]["type"],
        "paragraph"
    );
}

#[test]
fn c2_i_067_image_destination_balances_before_title() {
    assert_eq!(
        tree("![i](a(b.png \"t\")\n^ cap")["children"][0]["type"],
        "paragraph"
    );
}

#[test]
fn c2_b_197_image_reference_label_reads_escaped_bracket() {
    assert_eq!(
        tree("![a][b\\]c]\n^ cap")["children"][0]["type"],
        "paragraph"
    );
    assert_eq!(tree("![a][b[c]\n^ cap")["children"][0]["type"], "paragraph");
}

#[test]
fn c2_b_197_invalid_image_forms_do_not_promote() {
    for src in [
        "![a][ ]\n^ cap",
        "![a][@x]\n^ cap",
        "![a][b %% c]\n^ cap",
        "![a %% b](x)\n^ cap",
    ] {
        let t = tree(src);
        assert_eq!(t["children"][0]["type"], "paragraph", "{src}");
    }
    assert_eq!(tree("![a][]\n^ cap")["children"][0]["type"], "paragraph");
    assert_eq!(
        tree("![`a %% b`](x)\n^ cap")["children"][0]["type"],
        "paragraph"
    );
}

#[test]
fn c2_b_108_comment_fence_keeps_blank_body_spaces() {
    let t = tree("%%%\n \n-");
    assert_eq!(t["children"][0]["value"], " \n-");
}

#[test]
fn c2_h_165_degraded_raw_inline_uses_plain_value() {
    let opts = Options {
        profile: carve2::options::Profile::Comment,
        ..Options::default()
    };
    assert_eq!(render("a `<br>`{=html} b", &opts), "<p>a &lt;br&gt; b</p>");
}

#[test]
fn c2_h_165_degraded_table_omits_span_mark_cells() {
    let opts = Options {
        profile: carve2::options::Profile::Minimal,
        ..Options::default()
    };
    assert_eq!(render("| 1 | 2 | < |", &opts), "<p>1 | 2</p>");
}
