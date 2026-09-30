use carve::djot_to_carve;
use carve::to_html;

#[test]
fn emphasis_uses_djot_closer_ownership() {
    for (source, expected) in [
        ("foo*bar*baz", "<p>foo<strong>bar</strong>baz</p>"),
        ("_(_foo_)_", "<p><em>(</em>foo<em>)</em></p>"),
        ("___", "<p>___</p>"),
        ("_}b_", "<p>_}b_</p>"),
        ("_[bar_](url)", "<p><em>[bar</em>](url)</p>"),
        (
            "(some text){.attr}",
            "<p>(some <span class=\"attr\">text)</span></p>",
        ),
        ("~_x_~", "<p><sub><em>x</em></sub></p>"),
        ("^_x_^", "<p><sup><em>x</em></sup></p>"),
        ("_a_+ b", "<p><em>a</em>+ b</p>"),
        ("_a {.c}_", "<p><em>a </em></p>"),
        ("*a {.c}*", "<p><strong>a </strong></p>"),
        ("[r]: /u_v\n\n[x][r]", "<p><a href=\"/u_v\">x</a></p>"),
        ("![alt_x](u.png)", "<img src=\"u.png\" alt=\"alt_x\">"),
        ("a\n{.c}\nb", "<p>a\nb</p>"),
        ("_emph_{.a}", "<p><em class=\"a\">emph</em></p>"),
    ] {
        assert_eq!(to_html(&djot_to_carve(source)).trim(), expected, "{source}");
    }
}

#[test]
fn closed_blocks_leave_attributes_pending() {
    for block in ["```\nx\n```", "::: box\nx\n:::", "***", "| a |", "[r]: /u"] {
        let converted = djot_to_carve(&format!("{block}\n{{.c}}\npara"));
        assert!(
            to_html(&converted).contains("<p class=\"c\">para</p>"),
            "{block}: {converted}"
        );
    }
}

#[test]
fn deep_same_kind_spans_do_not_use_the_call_stack() {
    let source = format!("{}x{}", "{_".repeat(10000), "_}".repeat(10000));
    assert_eq!(djot_to_carve(&source), "{/x/}");
}

#[test]
fn raw_and_held_fences_keep_literal_delimiters() {
    let raw = djot_to_carve("``` =html\n<b>_x</b>\n```");
    assert_eq!(to_html(&raw).trim(), "<b>_x</b>");
    for fence in ["```", "~~~"] {
        let held = djot_to_carve(&format!(": {fence}\n  a_b\n  {fence}"));
        assert!(
            to_html(&held).contains("<pre><code>a_b\n</code></pre>"),
            "{held}"
        );
    }
}
