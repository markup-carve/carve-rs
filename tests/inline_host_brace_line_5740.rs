//! A brace group hides its backticks from the brace-line test only when inline
//! pairing attaches it to a host (carve#5740, CARVE-P2-035, PART 9 §9 R3).

fn html(source: &str) -> String {
    carve::to_html(source).trim().to_owned()
}

#[test]
fn a_brace_group_without_a_host_opens_a_code_span() {
    for host in ["a", "a*b*", "\\[a]"] {
        let text = host.replace('\\', "");
        assert_eq!(
            html(&format!("{host}{{title=\"`\"}}\n{{k=v}}\nb\n")),
            format!("<p>{text}{{title=“<code>\"}}\n{{k=v}}\nb</code></p>"),
            "{host}"
        );
    }
}

#[test]
fn a_brace_group_inside_an_item_follows_the_same_rule() {
    assert_eq!(
        html("- a{title=\"`\"}\n  {k=v}\n  b\n"),
        "<ul>\n  <li>a{title=“<code>\"}\n{k=v}\nb</code></li>\n</ul>"
    );
}

#[test]
fn an_attached_attribute_block_keeps_its_backtick() {
    assert_eq!(
        html("[a]{title=\"`\"}\n{k=v}\nb\n"),
        "<p><span title=\"`\">a</span></p>\n<p k=\"v\">b</p>"
    );
    assert_eq!(
        html("*a*{title=\"`\"}\n{k=v}\nb\n"),
        "<p><strong title=\"`\">a</strong></p>\n<p k=\"v\">b</p>"
    );
}

#[test]
fn a_host_opened_on_an_earlier_line_still_attaches() {
    assert_eq!(
        html("> *a\n> b*{title=\"`\"}\n> {k=v}\n> b\n"),
        "<blockquote>\n  <p><strong title=\"`\">a\nb</strong></p>\n  <p k=\"v\">b</p>\n</blockquote>"
    );
}
