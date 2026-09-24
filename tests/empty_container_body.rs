use carve::to_html;

#[test]
fn an_empty_bare_div_keeps_its_html_body_line() {
    for source in [":::\n:::\n", ":::\n%% hidden\n:::\n"] {
        assert_eq!(to_html(source), "<div>\n\n</div>");
    }
}

#[test]
fn every_empty_special_container_keeps_its_html_body_line() {
    for (source, expected) in [
        ("::: |\n:::\n", "<div class=\"line-block\">\n\n</div>"),
        ("::: \\\n:::\n", "<div class=\"hardbreaks\">\n\n</div>"),
        (
            "::: figure\n:::\n",
            "<figure class=\"carve-figure-group\">\n\n</figure>",
        ),
        (
            "::: \\\n%% hidden\n:::\n",
            "<div class=\"hardbreaks\">\n\n</div>",
        ),
        (
            "::: figure\n%% hidden\n:::\n",
            "<figure class=\"carve-figure-group\">\n\n</figure>",
        ),
    ] {
        assert_eq!(to_html(source), expected);
    }
}
