//! A bracket run bounds emphasis, so a kind open outside it opens again inside
//! (carve-rs#3487). A guard `_` or `/` that opens a span reads as START, so the
//! span after it takes its attribute block (carve-rs#3494). An editorial
//! comment hides the closer of an insertion or deletion (carve-rs#3495).

fn html(src: &str) -> String {
    carve::to_html(src).trim_end().to_string()
}

#[test]
fn a_bracket_run_hides_an_enclosing_kind() {
    assert_eq!(
        html("*[*g*] x*\n"),
        "<p><strong>[<strong>g</strong>] x</strong></p>"
    );
    assert_eq!(
        html("{*a [*g*] x*}\n"),
        "<p><strong>a [<strong>g</strong>] x</strong></p>"
    );
    assert_eq!(
        html("/*[/*g*/] x*/\n"),
        "<p><strong><em>[<strong><em>g</em></strong>] x</em></strong></p>"
    );
    assert_eq!(
        html("_a [_g_](u) x_\n"),
        "<p><u>a <a href=\"u\"><u>g</u></a> x</u></p>"
    );
}

#[test]
fn a_span_behind_a_guard_opener_takes_its_attribute_block() {
    assert_eq!(
        html("/_a_{title=\"/\"} b/\n"),
        "<p><em><u title=\"/\">a</u> b</em></p>"
    );
    assert_eq!(
        html("_*a*{title=\"_\"} b_\n"),
        "<p><u><strong title=\"_\">a</strong> b</u></p>"
    );
    assert_eq!(
        html("/_*a*{title=\"/\"}_ b/\n"),
        "<p><em><u><strong title=\"/\">a</strong></u> b</em></p>"
    );
}

#[test]
fn an_editorial_comment_hides_an_editorial_closer() {
    assert_eq!(
        html("{-{#a-}b#}-}\n"),
        "<p><del><span class=\"critic-comment\">a-}b</span></del></p>"
    );
    assert_eq!(
        html("{+x {#a+}b#} c+}\n"),
        "<p><ins>x <span class=\"critic-comment\">a+}b</span> c</ins></p>"
    );
    assert_eq!(
        html("{-{#a-}\n"),
        "<p><del>{<span class=\"tag\"><strong>#a</strong></span></del></p>"
    );
    let row = "{-{#a-}b#}-} ";
    let expected = "<del><span class=\"critic-comment\">a-}b</span></del> ";
    assert_eq!(
        html(&row.repeat(1500)),
        format!("<p>{}</p>", expected.repeat(1500).trim_end())
    );
}

#[test]
fn a_glued_attribute_block_wins_over_a_later_comment_closer() {
    assert_eq!(html("`x`{#a}b#}\n"), "<p><code id=\"a\">x</code>b#}</p>");
    assert_eq!(html("{+x+}{#a}b#}\n"), "<p><ins id=\"a\">x</ins>b#}</p>");
}

#[test]
fn a_link_inside_a_substitution_inside_a_link_is_text() {
    assert_eq!(
        html("[x {~a~>[y](u)~} z](v)\n"),
        "<p><a href=\"v\">x <del>a</del><ins>y</ins> z</a></p>"
    );
    assert_eq!(
        html("[x {~a~><http://u>~} z](v)\n"),
        "<p><a href=\"v\">x <del>a</del><ins>http://u</ins> z</a></p>"
    );
}
