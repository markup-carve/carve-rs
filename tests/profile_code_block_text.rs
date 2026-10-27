use carve::{Options, Profile};

fn minimal_html(source: &str) -> String {
    carve::to_html_with_options(source, &Options::new().with_profile(Profile::minimal()))
}

#[test]
fn denied_code_block_keeps_content_without_fences() {
    assert_eq!(minimal_html("```\nx\n```"), "<p>x</p>");
    assert_eq!(minimal_html("~~~ rust\nx\n~~~"), "<p>x</p>");
    assert_eq!(minimal_html("```\nx\ny\n```"), "<p>x<br>\ny</p>");
}
