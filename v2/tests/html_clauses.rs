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
