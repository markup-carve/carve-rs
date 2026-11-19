//! Renderer clauses proved on small inputs.

fn html(src: &str) -> String {
    carve::render(src, &carve::Options::default())
}

#[test]
fn c2_h_168_static_math_keeps_the_core_span_in_every_host() {
    let cases = [
        (
            "a $`x<y` b",
            "<p>a <span class=\"math inline\" role=\"math\">\\(x&lt;y\\)</span> b</p>",
        ),
        (
            "{.m}\n$$`x`",
            "<p class=\"m\"><span class=\"math display\" role=\"math\">\\[x\\]</span></p>",
        ),
        (
            "> $`x`",
            "<blockquote><p><span class=\"math inline\" role=\"math\">\\(x\\)</span></p></blockquote>",
        ),
        (
            "| $`x` |",
            "<table>\n  <tbody>\n    <tr><td><span class=\"math inline\" role=\"math\">\\(x\\)</span></td></tr>\n  </tbody>\n</table>",
        ),
        (
            "$`x`{.c}",
            "<p><span class=\"math inline c\" role=\"math\">\\(x\\)</span></p>",
        ),
    ];
    let opts = carve::Options {
        mode: carve::options::Mode::Static,
        ..carve::Options::default()
    };
    for (source, expected) in cases {
        assert_eq!(carve::render(source, &opts), expected, "{source}");
        assert_eq!(carve::render(source, &opts), html(source), "{source}");
    }
}

#[test]
fn c2_h_202_note_and_mark_ids_share_the_document_namespace() {
    let heading = html("= fn1\n\na[^x]\n\n[^x]: n");
    assert!(heading.contains("id=\"fn1-2\""), "{heading}");
    assert!(heading.contains("href=\"#fn1-2\""), "{heading}");

    let authored = html("a^[x] and b[^n]{#fn1}\n\n[^n]: y");
    assert!(authored.contains("id=\"fn1-2\""), "{authored}");
    assert!(authored.contains("id=\"fn1\" href=\"#fn2\""), "{authored}");

    let mark = html("a^[x]{#fnref2} b^[y]");
    assert!(mark.contains("id=\"fnref2-2\""), "{mark}");
    assert!(mark.contains("href=\"#fnref2-2\""), "{mark}");

    let repeated = html("a[^x] b[^x]\n\n[^x]: n\n\n{#fnref1-2}\np");
    assert!(repeated.contains("id=\"fnref1-2-2\""), "{repeated}");
    assert!(repeated.contains("href=\"#fnref1-2-2\""), "{repeated}");
}

#[test]
fn c2_h_069_comment_only_line_inside_an_inline_node_takes_its_break() {
    assert_eq!(html("*a\n%%\nb*"), "<p><strong>a\nb</strong></p>");
}

#[test]
fn c2_h_069_a_line_with_other_content_keeps_both_breaks() {
    assert_eq!(
        html("`=`{={%%}\n=}"),
        "<p><code>=</code><mark>\n</mark></p>"
    );
}

#[test]
fn c2_h_176_u007f_is_replaced() {
    assert_eq!(html("a\u{7f}b"), "<p>a\u{fffd}b</p>");
}

#[test]
fn c2_h_050_escaped_raw_html_keeps_pre_attributes() {
    assert_eq!(
        html("{f}\n```=html\n<p>x</p>\n```"),
        "<pre f=\"\"><code class=\"language-html\">&lt;p&gt;x&lt;/p&gt;\n</code></pre>"
    );
}

#[test]
fn c2_h_069_braced_comment_keeps_same_line_space_in_inline_note() {
    let html = html("^[{%%} ]");
    assert!(html.contains("<p> <a href=\"#fnref1\""), "{html}");
}

#[test]
fn c2_h_006_display_math_quote_is_one_line() {
    assert_eq!(
        html("> $$`x`"),
        "<blockquote><p><span class=\"math display\" role=\"math\">\\[x\\]</span></p></blockquote>"
    );
}

#[test]
fn c2_h_020_block_image_item_keeps_its_own_line() {
    assert_eq!(
        html("- ![a](b.png)"),
        "<ul>\n  <li>\n    <img src=\"b.png\" alt=\"a\">\n  </li>\n</ul>"
    );
}

#[test]
fn c2_b_197_destination_with_space_is_no_block_image() {
    assert_eq!(
        html("![alt](  *****a.png)\n^ Cap"),
        "<p>![alt](  *****a.png)\n^ Cap</p>"
    );
}

#[test]
fn c2_b_010_c2_i_079_a_lone_cr_is_label_text() {
    assert!(html("x[^\r]\n\n[^\r]: n").contains("doc-noteref"));
}

fn under(profile: carve::options::Profile, src: &str) -> String {
    let opts = carve::Options {
        profile,
        ..carve::Options::default()
    };
    carve::render(src, &opts)
}

#[test]
fn c2_h_165_c2_h_007_a_degraded_block_in_a_tight_item_is_bare() {
    use carve::options::Profile;
    assert_eq!(
        under(Profile::Comment, "-   - = a"),
        "<ul>\n  <li>\n    <ul>\n      <li># a</li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn c2_h_165_a_degraded_block_that_leaves_no_text_is_dropped() {
    use carve::options::Profile;
    assert_eq!(under(Profile::Minimal, "> %%%% c"), "");
    assert_eq!(under(Profile::Minimal, "- >"), "<ul>\n  <li></li>\n</ul>");
}

#[test]
fn c2_h_165_unclosed_math_degrades_to_writer_spelling() {
    let o = carve::Options {
        profile: carve::options::Profile::Minimal,
        ..carve::Options::default()
    };
    assert_eq!(carve::render("$`", &o), "<p>$`</p>");
}

#[test]
fn c2_h_165_code_in_a_degraded_cell_uses_its_value() {
    let o = carve::Options {
        profile: carve::options::Profile::Comment,
        ..carve::Options::default()
    };
    assert_eq!(carve::render("|``a`b``|", &o), "<p>a`b</p>");
}

#[test]
fn c2_h_166_gap_g_020_children_at_the_bound_keep_their_breaks() {
    use carve::options::Profile;
    assert_eq!(
        under(Profile::Minimal, "- a\n%% c\n- b"),
        "<ul>\n  <li>a </li>\n  <li>b</li>\n</ul>"
    );
}

#[test]
fn c2_b_105_a_brace_block_glued_to_a_fence_language_is_no_attribute() {
    assert_eq!(
        html("```py{.x}\nc\n```"),
        "<pre><code class=\"language-py\">c\n</code></pre>"
    );
}

#[test]
fn c2_i_084_heading_lookup_uses_plain_text() {
    assert_eq!(
        html("# \\*a\\*\n\n# *a*\n\n[x][*a*]\n"),
        concat!(
            "<section id=\"a\">\n  <h1>*a*</h1>\n</section>\n",
            "<section id=\"a-2\">\n  <h1><strong>a</strong></h1>\n  <p><a href=\"#a-2\">x</a></p>\n</section>"
        )
    );
}

#[test]
fn c2_a_035_a_later_key_takes_the_first_place_in_a_long_block() {
    let keys: Vec<String> = (0..20).map(|i| format!("k{i}=a")).collect();
    let classes: Vec<String> = (0..18).map(|i| format!(".x{i}")).collect();
    let src = format!(
        "{{{} K3=b .c .d .c {} .x2}}\npara\n",
        keys.join(" "),
        classes.join(" ")
    );
    let attrs: Vec<String> = (0..20)
        .map(|i| format!("k{i}=\"{}\"", if i == 3 { "b" } else { "a" }))
        .collect();
    let names: Vec<String> = (0..18).map(|i| format!("x{i}")).collect();
    assert_eq!(
        html(&src),
        format!(
            "<p class=\"c d {}\" {}>para</p>",
            names.join(" "),
            attrs.join(" ")
        )
    );
}

#[test]
fn c2_h_163_an_input_over_max_length_is_refused() {
    use carve::options::Profile;
    let opts = carve::Options {
        profile: Profile::Minimal,
        ..carve::Options::default()
    };
    assert!(carve::try_render(&"a".repeat(10_000), &opts).is_ok());
    assert_eq!(
        carve::try_render(&"é".repeat(5_001), &opts),
        Err(carve::RenderError::TooLong {
            profile: "minimal",
            limit: 10_000
        })
    );
    assert_eq!(carve::render(&"a".repeat(10_001), &opts), "");
}

#[test]
fn c2_h_130_strict_losses_turns_each_loss_into_an_error() {
    use carve::options::Profile;
    let opts = carve::Options {
        profile: Profile::Comment,
        strict_losses: true,
        ..carve::Options::default()
    };
    assert_eq!(
        carve::try_render("![i](b.png) ![j](c.png)", &opts),
        Err(carve::RenderError::Losses(vec![
            "profile-construct-degraded",
            "profile-construct-degraded"
        ]))
    );
    assert_eq!(carve::try_render("plain", &opts), Ok("<p>plain</p>".into()));
}

#[test]
fn c2_b_030_gap_g_021_a_left_sibling_moves_the_list_column() {
    assert_eq!(
        html(" - a\n- b\n 2. c"),
        "<ul>\n  <li>a</li>\n  <li>b\n2. c</li>\n</ul>"
    );
    assert_eq!(
        html(" 9. a\n10. b\n 11. c"),
        "<ol start=\"9\">\n  <li>a</li>\n  <li>b\n11. c</li>\n</ol>"
    );
}

#[test]
fn c2_i_102_c2_h_019_a_paragraph_of_one_hard_break_is_written() {
    assert_eq!(html("\\"), "<p><br>\n</p>");
}

fn social() -> carve::Options {
    carve::Options {
        social: Some(carve::options::Social {
            mention_url: Some("https://m/{name}?x={name}".into()),
            tag_url: Some("https://t/{name}".into()),
        }),
        ..carve::Options::default()
    }
}

#[test]
fn c2_h_100_gap_g_022_social_makes_mentions_and_tags_links() {
    assert_eq!(
        carve::render("@ann #topic", &social()),
        concat!(
            "<p><a class=\"mention\" href=\"https://m/ann?x=ann\">@ann</a> ",
            "<a class=\"tag\" href=\"https://t/topic\">#topic</a></p>"
        )
    );
    assert_eq!(
        carve::render("[x @ann](/u)", &social()),
        "<p><a href=\"/u\">x <span class=\"mention\"><strong>@ann</strong></span></a></p>"
    );
}

#[test]
fn c2_h_126_without_social_a_mention_is_an_inert_span() {
    assert_eq!(
        html("@ann"),
        "<p><span class=\"mention\"><strong>@ann</strong></span></p>"
    );
}

#[test]
fn c2_b_184_an_empty_cell_takes_a_fragment_without_a_space() {
    assert_eq!(
        html("|  | x |\n+ b | y |"),
        "<table>\n  <tbody>\n    <tr><td>b</td><td>x y</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn c2_b_176_an_empty_fragment_past_the_last_column_forms_a_cell() {
    assert_eq!(
        html("| a |\n+ b | |"),
        "<table>\n  <tbody>\n    <tr><td>a b</td><td></td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn c2_b_192_attribute_lines_no_block_takes_stay_before_a_comment_line() {
    let doc = carve::parse("{a}\n%% x").unwrap();
    let kinds: Vec<&str> = doc
        .children
        .iter()
        .map(|n| match n {
            carve::ast::owned::Node::Paragraph(_) => "paragraph",
            carve::ast::owned::Node::Comment(_) => "comment",
            _ => "other",
        })
        .collect();
    assert_eq!(kinds, ["paragraph", "comment"]);
}

#[test]
fn c2_b_146_a_caret_line_the_item_does_not_own_is_a_lazy_line() {
    assert_eq!(
        html("- ![](a.png)\n^ cap"),
        "<ul>\n  <li><img src=\"a.png\" alt=\"\">\n^ cap</li>\n</ul>"
    );
}

#[test]
fn c2_b_167_a_row_takes_a_trailing_line_comment() {
    assert_eq!(
        html("| a | %% c\n| b |"),
        "<table>\n  <tbody>\n    <tr><td>a</td></tr>\n    <tr><td>b</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn c2_i_032_an_unquoted_value_may_hold_an_opening_brace() {
    assert_eq!(html("{s={}\nx"), "<p s=\"{\">x</p>");
}

#[test]
fn g_008_one_empty_last_line_of_a_delimited_body_is_no_record() {
    assert_eq!(
        html("{table}\n```csv\na\n\n\n```"),
        "<table>\n  <thead>\n    <tr><th scope=\"col\">a</th></tr>\n  </thead>\n  <tbody>\n    <tr><td></td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn c2_b_082_a_footnote_label_outside_ascii_keeps_its_body() {
    assert!(html("[^\u{e9}]: T\n\n[^\u{e9}]").contains("<p>T<a href=\"#fnref1\""));
}

#[test]
fn g_024_a_cell_that_spans_columns_takes_no_row_span() {
    assert_eq!(
        html("|a|<|\n|^|b|"),
        "<table>\n  <tbody>\n    <tr><td colspan=\"2\">a</td></tr>\n    <tr><td>^</td><td>b</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn g_004_the_block_inside_a_figure_starts_at_its_own_markup() {
    let doc = carve::parse("> {l}\n> ```\n> ```\n> ^ i").unwrap();
    let Some(carve::ast::owned::Node::BlockQuote(q)) = doc.children.first() else {
        panic!("no quote");
    };
    let Some(carve::ast::owned::Node::CodeBlock(f)) = q.children.first() else {
        panic!("no captioned code block");
    };
    assert!(f.caption.is_some());
    assert_eq!(f.pos.map(|p| p.start_offset), Some(2));
}

#[test]
fn g_025_trailing_blank_lines_of_an_unclosed_line_block_are_dropped() {
    let doc = carve::parse("::: |\na\n\n\n").unwrap();
    let Some(carve::ast::owned::Node::LineBlock(b)) = doc.children.first() else {
        panic!("no line block");
    };
    assert_eq!(b.children.len(), 1);
    let doc = carve::parse("::: |\na\n\nb\n:::").unwrap();
    let Some(carve::ast::owned::Node::LineBlock(b)) = doc.children.first() else {
        panic!("no line block");
    };
    assert_eq!(b.children.len(), 3);
}

#[test]
fn c2_b_052_three_blank_lines_end_a_description_list() {
    assert_eq!(
        html(":: t\n\n\n\n:: u"),
        "<dl>\n  <dt>t</dt>\n</dl>\n<dl>\n  <dt>u</dt>\n</dl>"
    );
}

#[test]
fn g_026_a_caret_line_below_a_quoted_term_is_a_lazy_line() {
    assert_eq!(
        html("> :: t\n^ c"),
        "<blockquote>\n  <dl>\n    <dt>t\n^ c</dt>\n  </dl>\n</blockquote>"
    );
}
