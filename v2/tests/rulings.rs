use carve2::{Options, render};

fn tree(src: &str) -> serde_json::Value {
    let arena = carve2::Arena::new();
    let document = carve2::parse_resolved_in(src, &Options::default(), &arena);
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
fn c2_b_205_shift_has_at_most_nine_digits() {
    assert_eq!(
        render(
            "{{ a.crv @shift:-123456789 }}\n\n{{ a.crv @shift:1234567890 }}",
            &Options::default()
        ),
        "<p>{{ a.crv @shift:-123456789 }}</p>\n<p>{{ a.crv <span class=\"mention\"><strong>@shift</strong></span>:1234567890 }}</p>"
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
fn c2_h_020_attributed_block_image_keeps_paragraph() {
    let src = "{.c #i}\n![a](x.png)\n\n{:de}\n![b](y.png)\n\n![c](z.png)";
    assert_eq!(
        render(src, &Options::default()),
        concat!(
            "<p id=\"i\" class=\"c\"><img src=\"x.png\" alt=\"a\"></p>\n",
            "<p lang=\"de\"><img src=\"y.png\" alt=\"b\"></p>\n",
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
        "<p>## Sub em code</p>\n<p>[Sub em code][] and &lt;/#Sub-em-code&gt;</p>"
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
            "<p><a class=\"mention\" href=\"https://x.org/u/ann.b\"><strong>@ann.b</strong></a>",
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
fn c2_i_126_capped_bracket_opener_never_takes_a_closer() {
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
fn c2_a_158_caption_mark_and_empty_or_span_cells_have_positions() {
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
    assert!(
        rows[1]["children"][1]["children"][0]["children"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        rows[2]["children"][0]["children"][0]["children"]
            .as_array()
            .unwrap()
            .is_empty()
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
