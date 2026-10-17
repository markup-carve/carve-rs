use carve::{to_html, to_html_with_options, Options};

#[test]
fn a_description_host_keeps_the_outer_item_after_the_inner_item_ends() {
    let source = ":: t\n: - 1. para\n      %%%\n      c\n      %%%\n       tail\n";
    for html in [
        to_html(source),
        to_html_with_options(source, &Options::default().with_positions(true)),
    ] {
        let compact = html.lines().map(str::trim).collect::<Vec<_>>().join("\n");
        assert!(
            compact.contains("<li>para</li>\n</ol>\ntail\n</li>"),
            "{html}"
        );
    }
}
