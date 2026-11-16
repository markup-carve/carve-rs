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
    let input = format!("{}*:n[[]", "> ".repeat(198));
    let html = carve2::render(&input, &Options::default());
    assert!(html.contains("<p>*:n[[]</p>"), "{html}");
    let capped = format!("{}*:n[a]", "> ".repeat(199));
    let html = carve2::render(&capped, &Options::default());
    assert!(html.contains("<p>*:n[a]</p>"), "{html}");
    let fits = format!("{}*:n[a]", "> ".repeat(198));
    let html = carve2::render(&fits, &Options::default());
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
fn c2_h_165_degraded_block_image_uses_alt_text() {
    let opts = Options {
        profile: Profile::Minimal,
        ..Options::default()
    };
    assert_eq!(carve2::render("> ![i](a.png)", &opts), "<p>&gt; i</p>");
}

#[test]
fn c2_i_118_enclosing_link_counts_when_inner_link_forms() {
    let opts = Options::default();
    let nested = format!("{}[*[x](u)](v)", "> ".repeat(198));
    let html = carve2::render(&nested, &opts);
    assert!(html.contains("<a href=\"v\">*[x](u)</a>"), "{html}");
    let image = format!("{}[*![i](u)](v)", "> ".repeat(198));
    let html = carve2::render(&image, &opts);
    assert!(html.contains("<a href=\"v\">*![i](u)</a>"), "{html}");
    let extension = format!("{}:n[*[x](u)]", "> ".repeat(198));
    let html = carve2::render(&extension, &opts);
    assert!(
        html.contains("<span class=\"ext-n\">*[x](u)</span>"),
        "{html}"
    );
    let unclosed = format!("{}[*[x](u)", "> ".repeat(198));
    let html = carve2::render(&unclosed, &opts);
    assert!(html.contains("<a href=\"u\">x</a>"), "{html}");
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
