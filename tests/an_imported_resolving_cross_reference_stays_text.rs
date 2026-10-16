use carve::{html_to_carve, markdown_to_carve, to_html, HtmlImportOptions};

fn assert_literal_reference(carve: &str, id: &str) {
    let html = to_html(carve);
    assert!(html.contains(&format!("&lt;/#{id}&gt;")), "{html}");
    assert!(!html.contains(&format!("<a href=\"#{id}\">")), "{html}");
}

#[test]
fn markdown_heading_id_resolves_only_with_exact_case() {
    let carve = markdown_to_carve("# Intro\n\nsee </#intro> and </#Intro> here\n");
    assert_eq!(carve, "# Intro\n\nsee </#intro> and </\\#Intro> here\n");
    assert_literal_reference(&carve, "Intro");
}

#[test]
fn html_heading_id_resolves_only_with_exact_case() {
    let carve = html_to_carve(
        "<h1>Intro</h1>\n<p>see &lt;/#intro&gt; and &lt;/#Intro&gt; here</p>\n",
        &HtmlImportOptions::default(),
    )
    .expect("HTML import")
    .value;
    assert_eq!(carve, "# Intro\n\nsee </#intro> and </\\#Intro> here\n");
    assert_literal_reference(&carve, "Intro");
}

#[test]
fn explicit_html_heading_id_stays_literal() {
    let carve = html_to_carve(
        "<h1 id=\"intro\">Intro</h1>\n<p>see &lt;/#intro&gt; here</p>\n",
        &HtmlImportOptions::default(),
    )
    .expect("HTML import")
    .value;
    assert_eq!(carve, "{#intro}\n# Intro\n\nsee </\\#intro> here\n");
    assert_literal_reference(&carve, "intro");
}
