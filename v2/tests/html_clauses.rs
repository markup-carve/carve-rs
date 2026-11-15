//! Renderer clauses proved on small inputs.

fn html(src: &str) -> String {
    carve2::render(src, &carve2::Options::default())
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

fn under(profile: carve2::options::Profile, src: &str) -> String {
    let opts = carve2::Options {
        profile,
        ..carve2::Options::default()
    };
    carve2::render(src, &opts)
}

#[test]
fn c2_h_165_c2_h_007_a_degraded_block_in_a_tight_item_is_bare() {
    use carve2::options::Profile;
    assert_eq!(
        under(Profile::Comment, "-   - = a"),
        "<ul>\n  <li>\n    <ul>\n      <li># a</li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn c2_h_165_a_degraded_block_that_leaves_no_text_is_dropped() {
    use carve2::options::Profile;
    assert_eq!(under(Profile::Minimal, "> %%%% c"), "");
    assert_eq!(under(Profile::Minimal, "- >"), "<ul>\n  <li></li>\n</ul>");
}

#[test]
fn c2_h_166_gap_g_020_children_at_the_bound_keep_their_breaks() {
    use carve2::options::Profile;
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
fn c2_i_084_the_first_heading_matching_by_text_or_source_wins() {
    assert_eq!(
        html("# \\*a\\*\n\n# *a*\n\n[x][*a*]\n"),
        "<section id=\"a\">\n  <h1>*a*</h1>\n</section>\n<section id=\"a-2\">\n  <h1><strong>a</strong></h1>\n  <p><a href=\"#a\">x</a></p>\n</section>"
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
    use carve2::options::Profile;
    let opts = carve2::Options {
        profile: Profile::Minimal,
        ..carve2::Options::default()
    };
    assert!(carve2::try_render(&"a".repeat(10_000), &opts).is_ok());
    assert_eq!(
        carve2::try_render(&"é".repeat(5_001), &opts),
        Err(carve2::RenderError::TooLong {
            profile: "minimal",
            limit: 10_000
        })
    );
    assert_eq!(carve2::render(&"a".repeat(10_001), &opts), "");
}

#[test]
fn c2_h_130_strict_losses_turns_each_loss_into_an_error() {
    use carve2::options::Profile;
    let opts = carve2::Options {
        profile: Profile::Comment,
        strict_losses: true,
        ..carve2::Options::default()
    };
    assert_eq!(
        carve2::try_render("![i](b.png) ![j](c.png)", &opts),
        Err(carve2::RenderError::Losses(vec![
            "profile-construct-degraded",
            "profile-construct-degraded"
        ]))
    );
    assert_eq!(
        carve2::try_render("plain", &opts),
        Ok("<p>plain</p>".into())
    );
}
