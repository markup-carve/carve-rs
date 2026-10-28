use carve::{html_to_ast, html_to_carve, parse, render_html, to_json, HtmlImportOptions};

#[test]
fn image_only_imports_use_the_resolved_paragraph_shape() {
    let options = HtmlImportOptions::default();
    let hosts = [
        ("", ""),
        ("<div>", "</div>"),
        ("<blockquote>", "</blockquote>"),
        ("<ul><li>", "</li></ul>"),
        ("<section>", "</section>"),
    ];
    let mut checked = 0;
    for (open, close) in hosts {
        for source in 0..10 {
            for alt in 0..10 {
                for sibling in ["", "<p>tail</p>"] {
                    let html = format!(
                        "{open}<img src=\"img{source}.png\" alt=\"alt{alt}\">{sibling}{close}"
                    );
                    let imported = html_to_ast(&html, &options).unwrap();
                    let written = html_to_carve(&html, &options).unwrap();
                    assert_eq!(
                        render_html(&imported.value).unwrap(),
                        render_html(&parse(&written.value)).unwrap(),
                        "{html}"
                    );
                    assert!(
                        to_json(&imported.value).contains("\"blockImage\":true"),
                        "{html}"
                    );
                    checked += 1;
                }
            }
        }
    }
    assert_eq!(checked, 1_000);
}
