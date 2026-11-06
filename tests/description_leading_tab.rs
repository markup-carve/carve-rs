use carve::to_html;

#[test]
fn description_keeps_tab_after_marker_space() {
    let source = ":: t\n: \ttext\n\nflush\n";
    assert_eq!(
        to_html(source),
        "<dl>\n  <dt>t</dt>\n  <dd>\ttext</dd>\n</dl>\n<p>flush</p>"
    );
    let ast: serde_json::Value =
        serde_json::from_str(&carve::to_json(&carve::parse(source))).expect("valid AST JSON");
    assert_eq!(
        ast["children"][0]["items"][1]["children"][0]["children"][0]["value"],
        "\ttext"
    );
    let formatted = carve::to_carve(source);
    assert!(formatted.contains(": \ttext"), "{formatted:?}");
    assert_eq!(to_html(&formatted), to_html(source));
}

#[test]
fn tab_leading_description_variants_keep_the_tab() {
    for n in 0..1000 {
        let source = format!(":: term{n}\n: \tword{n}\n");
        let html = to_html(&source);
        assert!(
            html.contains(&format!("<dd>\tword{n}</dd>")),
            "variant {n}: {html}"
        );
    }
}
