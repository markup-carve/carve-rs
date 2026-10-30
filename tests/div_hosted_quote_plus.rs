use carve::to_html;

fn compact(html: &str) -> String {
    html.lines().map(str::trim).collect::<String>()
}

#[test]
fn consecutive_plus_stays_in_div_hosted_quote() {
    for host in ["note", "tip", "custom"] {
        let source = format!("::: {host}\n> q\n+\n+\n:::\n");
        let html = compact(&to_html(&source));
        assert!(
            html.contains("<blockquote><p>q</p><p>+</p></blockquote>"),
            "{source:?}: {html}"
        );

        let shifted = format!("::: {host}\n> q\n +\n+\n:::\n");
        let html = to_html(&shifted);
        assert!(
            html.contains("<blockquote><p>q\n+\n+</p></blockquote>"),
            "{shifted:?}: {html}"
        );

        let followed = format!("::: {host}\n> q\n+\n+\nafter\n:::\n");
        let html = to_html(&followed);
        assert!(html.contains("<p>q</p>"), "{followed:?}: {html}");
        assert!(html.contains("<p>after</p>"), "{followed:?}: {html}");
    }
}
