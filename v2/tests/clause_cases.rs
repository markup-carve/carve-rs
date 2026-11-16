use carve2::options::{Options, Profile};

#[test]
fn c2_b_105_glued_div_attribute_is_not_applied() {
    let input = "::: word{k=v}\ntext\n:::";
    assert_eq!(
        carve2::render(input, &Options::default()),
        "<div class=\"word\">\n  <p>text</p>\n</div>"
    );
}

#[test]
fn c2_i_118_open_bracket_and_delimiter_counts() {
    let input = format!("{}*:n[a]", "> ".repeat(199));
    let html = carve2::render(&input, &Options::default());
    assert!(
        html.contains("<p>*<span class=\"ext-n\">a</span></p>"),
        "{html}"
    );
}

#[test]
fn c2_h_165_degraded_block_uses_plain_text() {
    let opts = Options {
        profile: Profile::Minimal,
        ..Options::default()
    };
    let input = "> a <b:c> {#x#} d";
    assert_eq!(carve2::render(input, &opts), "<p>&gt; a b:c  d</p>");
}

#[test]
fn c2_h_166_degraded_paragraph_in_kept_item() {
    let opts = Options {
        profile: Profile::Minimal,
        ..Options::default()
    };
    let input = "- - a `b` <c:d>";
    let expected = "<ul>\n  <li>\n    <ul>\n      <li>a b c:d</li>\n    </ul>\n  </li>\n</ul>";
    assert_eq!(carve2::render(input, &opts), expected);
}

#[test]
fn c2_h_051_raw_html_passes_without_carrier() {
    let opts = Options {
        roundtrip: true,
        raw_html: true,
        ..Options::default()
    };
    assert_eq!(
        carve2::render("`<b>x</b>`{=html}", &opts),
        "<p><b>x</b></p>"
    );
}

#[test]
fn c2_b_010_crlf_dropped_and_lone_cr_kept_in_degraded_text() {
    let opts = Options {
        profile: Profile::Minimal,
        ..Options::default()
    };
    assert_eq!(carve2::render("> a\r\nb", &opts), "<p>&gt; a b</p>");
    assert_eq!(carve2::render("> a\rb", &opts), "<p>&gt; a\rb</p>");
}

#[test]
fn r13_undecided_child_source_reading_is_unchanged() {
    let opts = Options {
        profile: Profile::Minimal,
        ..Options::default()
    };
    let input = "[a `<br>`{=html}](u) b";
    assert_eq!(
        carve2::render(input, &opts),
        "<p>a `&lt;br&gt;`{=html} b</p>"
    );
}

#[test]
fn c2_h_016_r11_kept_nested_item_has_source_line() {
    let opts = Options {
        profile: Profile::Minimal,
        source_lines: true,
        ..Options::default()
    };
    let html = carve2::render("- - -C2-A-034", &opts);
    assert!(
        html.contains("<li data-source-line=\"1\">-C2-A-034</li>"),
        "{html}"
    );
}

#[test]
fn c2_h_165_childless_raw_uses_writer_spelling() {
    let article = Options {
        profile: Profile::Article,
        ..Options::default()
    };
    assert_eq!(carve2::render("`x`{=html}", &article), "<p>`x`{=html}</p>");
}

#[test]
fn c2_b_119_tab_after_space_opens_anonymous_div() {
    let input = ":::: \tx\nz\n::::";
    assert_eq!(
        carve2::render(input, &Options::default()),
        "<div>\n  <p>z</p>\n</div>"
    );
}

#[test]
fn c2_i_023_line_comment_after_tab_accepts_combining_mark() {
    let input = "a\t%%́x";
    assert_eq!(carve2::render(input, &Options::default()), "<p>a</p>");
}

#[test]
fn c2_h_181_supplied_abbreviation_line_break_is_spelled_as_space() {
    let opts = Options {
        roundtrip: true,
        ..Options::default()
    };
    let arena = carve2::Arena::new();
    let doc = carve2::parse_resolved_in("*[A]: A B", &opts, &arena).unwrap();
    let mut tree: serde_json::Value = serde_json::from_str(&carve2::to_json(&doc)).unwrap();
    for expansion in ["A\nB", "A\r\nB"] {
        tree["children"][0]["expansion"] = serde_json::Value::String(expansion.to_string());
        let carve2::ast::owned::Node::Document(doc) = serde_json::from_value(tree.clone()).unwrap()
        else {
            panic!("expected a document");
        };
        let (html, losses) = carve2::html::render_with_losses(&doc.in_arena(&arena), &opts);
        assert_eq!(
            html,
            "<template data-carve-abbreviations>*[A]: A B</template>"
        );
        assert_eq!(losses, ["field-unspellable"]);
    }
}

#[test]
fn c2_w_077_degraded_child_code_uses_shortest_absent_run() {
    let opts = Options {
        profile: Profile::Comment,
        ..Options::default()
    };
    assert_eq!(
        carve2::render("*{+{-{=```a``b```=}-}+}*", &opts),
        "<p><strong><ins><del><mark>`a``b`</mark></del></ins></strong></p>"
    );
}

#[test]
fn c2_w_117_supplied_abbreviation_trailing_space_reports_loss() {
    let opts = Options {
        roundtrip: true,
        ..Options::default()
    };
    let arena = carve2::Arena::new();
    let doc = carve2::parse_resolved_in("*[A]: A", &opts, &arena).unwrap();
    let mut tree: serde_json::Value = serde_json::from_str(&carve2::to_json(&doc)).unwrap();
    tree["children"][0]["expansion"] = serde_json::Value::String("A ".to_string());
    let carve2::ast::owned::Node::Document(doc) = serde_json::from_value(tree).unwrap() else {
        panic!("expected a document");
    };
    let (_, losses) = carve2::html::render_with_losses(&doc.in_arena(&arena), &opts);
    assert_eq!(losses, ["field-unspellable"]);
}

#[test]
fn c2_h_165_degraded_block_image_uses_fallback_spelling() {
    let opts = Options {
        profile: Profile::Minimal,
        ..Options::default()
    };
    assert_eq!(
        carve2::render("> ![i](a.png)", &opts),
        "<p>&gt; [img: i]</p>"
    );
}

#[test]
fn c2_a_092_joined_span_mark_discards_its_prefix_and_position() {
    let arena = carve2::Arena::new();
    let doc = carve2::parse_resolved_in("|= H |\n|={.x} ^ |\n| a |", &Options::default(), &arena)
        .unwrap();
    let tree: serde_json::Value = serde_json::from_str(&carve2::to_json(&doc)).unwrap();
    let mark = &tree["children"][0]["children"][0]["children"][1]["children"][0];
    assert_eq!(mark["header"], false);
    assert_eq!(mark["span"], "rowspan");
    assert_eq!(mark["children"], serde_json::json!([]));
    for field in ["attrs", "lang", "align", "pos"] {
        assert!(mark.get(field).is_none(), "{field}: {mark}");
    }
}

#[test]
fn c2_a_082_code_caption_skips_sibling_comment() {
    let input = "```js\nx\n```\n%% c\n^ Listing #: L";
    let arena = carve2::Arena::new();
    let doc = carve2::parse_resolved_in(input, &Options::default(), &arena).unwrap();
    let tree: serde_json::Value = serde_json::from_str(&carve2::to_json(&doc)).unwrap();
    assert_eq!(tree["children"][0]["type"], "code_block");
    assert_eq!(tree["children"][0]["caption"]["type"], "caption");
    assert_eq!(tree["children"][1]["type"], "comment");
    assert!(
        carve2::render(input, &Options::default())
            .contains("<figcaption>Listing 1: L</figcaption>")
    );
}

#[test]
fn c2_h_062_captioned_image_in_single_description_keeps_figure() {
    let html = carve2::render(":: t\n: ![a](x.png)\n  ^ cap", &Options::default());
    assert!(html.contains("<dd>\n    <figure>"), "{html}");
    assert!(html.contains("<figcaption>cap</figcaption>"), "{html}");
}

#[test]
fn c2_h_165_nested_degradation_keeps_captions() {
    let opts = Options {
        profile: Profile::Minimal,
        ..Options::default()
    };
    let list = carve2::render("- ![a](x.png)\n  ^ cap", &opts);
    assert!(list.contains("[img: a]"), "{list}");
    assert!(list.contains("cap"), "{list}");
    let quote = carve2::render("> > q\n> ^ cap", &opts);
    assert!(quote.contains("cap"), "{quote}");
}

#[test]
fn c2_b_174_head_test_ignores_unmarked_joining_caret() {
    let input = "|= H |= A |\n|= B | ^ |\n| 1 | 2 |";
    let arena = carve2::Arena::new();
    let doc = carve2::parse_resolved_in(input, &Options::default(), &arena).unwrap();
    let tree: serde_json::Value = serde_json::from_str(&carve2::to_json(&doc)).unwrap();
    let head = &tree["children"][0]["children"][0];
    assert_eq!(head["type"], "table_head");
    assert_eq!(head["children"].as_array().unwrap().len(), 2);
    assert_eq!(head["children"][0]["children"][1]["rowspan"], 2);
}

#[test]
fn c2_b_174_head_test_counts_blocked_less_than_mark() {
    let input = "|= a |= b |\n|= ^ | < |\n| 1 | 2 |";
    let html = carve2::render(input, &Options::default());
    assert!(html.contains("<tbody>"), "{html}");
    assert!(
        html.contains("<th colspan=\"2\" scope=\"row\">^</th>"),
        "{html}"
    );
}

#[test]
fn c2_h_016_inner_captioned_quote_uses_its_own_source_line() {
    let opts = Options {
        source_lines: true,
        ..Options::default()
    };
    let html = carve2::render("{.k}\n> q\n^ cap", &opts);
    assert!(
        html.contains("<figure class=\"k\" data-source-line=\"1\">"),
        "{html}"
    );
    assert!(
        html.contains("<blockquote data-source-line=\"2\">"),
        "{html}"
    );
}

#[test]
fn c2_b_137_comment_only_line_does_not_separate_image_caption() {
    let input = "![a](x.png)\n%% c\n^ cap";
    let arena = carve2::Arena::new();
    let doc = carve2::parse_resolved_in(input, &Options::default(), &arena).unwrap();
    let tree: serde_json::Value = serde_json::from_str(&carve2::to_json(&doc)).unwrap();
    assert_eq!(tree["children"][0]["caption"]["type"], "caption");
    assert!(carve2::render(input, &Options::default()).contains("<figcaption>cap</figcaption>"));
}

#[test]
fn c2_i_006_unformed_bracket_does_not_bound_delimiter_pairing() {
    let input = format!("{}[.*.]", "*a ".repeat(199));
    let html = carve2::render(&input, &Options::default());
    assert!(html.ends_with("<strong>a [.</strong>.]</p>"), "{html}");
}

#[test]
fn c2_i_006_unclosed_bracket_does_not_bound_delimiter_pairing() {
    let input = "*a ".repeat(197) + "/}*{/[{_^}b{/*}";
    let html = carve2::render(&input, &Options::default());
    assert!(
        html.ends_with("<strong>a <strong>a /}</strong>{/[{_^}b{/</strong>}</p>"),
        "{html}"
    );
}

#[test]
fn c2_h_006_captioned_child_keeps_multiline_quote_when_caption_has_no_text() {
    let opts = Options {
        profile: Profile::Comment,
        ..Options::default()
    };
    assert_eq!(
        carve2::render("> ![a](x.png)\n> ^ {%c%}", &opts),
        "<blockquote>\n  <p>[img: a]</p>\n</blockquote>"
    );
}

#[test]
fn c2_i_118_enclosing_link_counts_when_inner_link_forms() {
    let input = format!("{}- *[x](u)", "> ".repeat(197));
    let html = carve2::render(&input, &Options::default());
    assert!(html.contains("<li>*<a href=\"u\">x</a></li>"), "{html}");
}

#[test]
fn c2_h_165_comment_only_paragraph_writes_nothing() {
    let opts = Options {
        profile: Profile::Minimal,
        ..Options::default()
    };
    assert_eq!(carve2::render("{#c#}", &opts), "");
    assert_eq!(carve2::render("> {#c#}", &opts), "");
    for input in ["{#c#}", "- - {#c#}", "- - - {#c#}"] {
        let arena = carve2::Arena::new();
        let doc = carve2::parse_resolved_in(input, &opts, &arena).unwrap();
        let (_, losses) = carve2::html::render_with_losses(&doc, &opts);
        assert_eq!(losses, ["profile-construct-degraded"], "{input}");
        let strict = Options {
            strict_losses: true,
            ..opts.clone()
        };
        assert!(
            matches!(
                carve2::try_render(input, &strict),
                Err(carve2::RenderError::Losses(_))
            ),
            "{input}"
        );
    }
}

#[test]
fn c2_h_165_degraded_list_drops_empty_items() {
    let opts = Options {
        profile: Profile::Minimal,
        ..Options::default()
    };
    assert_eq!(
        carve2::render("- - - {#c#}\n- b", &opts),
        "<ul>\n  <li>\n    <ul>\n    </ul>\n  </li>\n  <li>b</li>\n</ul>"
    );
}

#[test]
fn c2_h_062_captioned_math_at_end_of_footnote_keeps_figure() {
    let input = "x[^a]\n\n[^a]: n\n\n  $$`x`\n  ^ Eq";
    let html = carve2::render(input, &Options::default());
    assert!(html.contains("<figure>"), "{html}");
    assert!(html.contains("<figcaption>Eq</figcaption>"), "{html}");
}

#[test]
fn c2_b_147_raw_block_is_not_a_caption_target() {
    let input = "```=html\n<b>\n```\n^ cap";
    let arena = carve2::Arena::new();
    let doc = carve2::parse_resolved_in(input, &Options::default(), &arena).unwrap();
    let tree: serde_json::Value = serde_json::from_str(&carve2::to_json(&doc)).unwrap();
    assert_eq!(tree["children"][0]["type"], "raw_block");
    let html = carve2::render(input, &Options::default());
    assert!(html.contains("<p>^ cap</p>"), "{html}");
    assert!(!html.contains("<figcaption>"), "{html}");
}

#[test]
fn c2_a_082_comment_after_closed_figure_group_stays_inside_it() {
    let input = "::: figure\n![a](b)\n:::\n%% c\n^ cap";
    let arena = carve2::Arena::new();
    let doc = carve2::parse_resolved_in(input, &Options::default(), &arena).unwrap();
    let tree: serde_json::Value = serde_json::from_str(&carve2::to_json(&doc)).unwrap();
    let group = &tree["children"][0];
    assert_eq!(group["type"], "figure_group");
    assert_eq!(
        group["children"].as_array().unwrap().last().unwrap()["type"],
        "comment"
    );
    assert_eq!(group["caption"]["type"], "caption");
}

#[test]
fn c2_a_082_comment_after_closed_quote_fence_stays_inside_it() {
    let input = "::: >\nq\n:::\n%% c\n^ cap";
    let arena = carve2::Arena::new();
    let doc = carve2::parse_resolved_in(input, &Options::default(), &arena).unwrap();
    let tree: serde_json::Value = serde_json::from_str(&carve2::to_json(&doc)).unwrap();
    let quote = &tree["children"][0];
    assert_eq!(quote["type"], "block_quote");
    assert_eq!(
        quote["children"].as_array().unwrap().last().unwrap()["type"],
        "comment"
    );
    assert_eq!(quote["caption"]["type"], "caption");
}

#[test]
fn c2_b_184_continuation_below_blocked_less_than_stays_in_its_cell() {
    let input = "| a | b |\n| ^ | < |\n+ x | y |";
    let html = carve2::render(input, &Options::default());
    assert!(html.contains("<td>&lt; y</td>"), "{html}");
    assert!(!html.contains("a x y"), "{html}");
}

#[test]
fn c2_b_184_continuation_below_joining_less_than_moves_to_origin() {
    let html = carve2::render("| a | < |\n+ x | y |", &Options::default());
    assert!(html.contains("<td colspan=\"2\">a x y</td>"), "{html}");
}

#[test]
fn c2_i_026_comment_transparency_does_not_spend_depth_on_a_failed_opener() {
    let input = format!("{}*{{%c%}} *x*", "*a ".repeat(199));
    let html = carve2::render(&input, &Options::default());
    assert!(html.contains("<strong>x</strong>"), "{html}");
}

#[test]
fn c2_b_010_lone_cr_in_abbreviation_expansion_is_content() {
    let opts = Options {
        roundtrip: true,
        ..Options::default()
    };
    let arena = carve2::Arena::new();
    let doc = carve2::parse_resolved_in("*[A]: A\rB", &opts, &arena).unwrap();
    let (html, losses) = carve2::html::render_with_losses(&doc, &opts);
    assert_eq!(
        html,
        "<template data-carve-abbreviations>*[A]: A\rB</template>"
    );
    assert!(losses.is_empty(), "{losses:?}");
}
