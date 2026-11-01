use carve::{
    expand_includes, parse, render_carve, render_html, to_html, to_json, IncludeContext,
    IncludeDenial, IncludeOptions, IncludeResolved,
};

#[test]
fn a_literal_directive_and_its_follower_are_two_paragraphs() {
    let source = "- {{ _x_ @code:js }}\n  ^ c\n";
    let expected = "<ul>\n  <li><p>{{ _x_ @code:js }}</p>\n    <p>^ c</p>\n  </li>\n</ul>";
    assert_eq!(to_html(source).trim(), expected);
    let doc = parse(source);
    assert_eq!(render_html(&doc).unwrap().trim(), expected);
    assert!(to_json(&doc).contains("\"tight\":false"));
    let written = render_carve(&doc).unwrap();
    assert_eq!(to_html(&written).trim(), expected);
}

#[test]
fn a_resolved_code_include_with_its_caption_stays_compact() {
    let source = "- {{ data.csv @code:csv }}\n  ^ cap\n";
    let resolver =
        |path: &str, _ctx: &IncludeContext<'_>| -> Result<IncludeResolved, IncludeDenial> {
            if path == "data.csv" {
                Ok(IncludeResolved::from("a\n".to_string()))
            } else {
                Err(IncludeDenial::NotFound)
            }
        };
    let result = expand_includes(
        parse(source),
        source,
        &IncludeOptions::new().with_resolver(&resolver),
    );
    let html = render_html(&result.doc).unwrap();
    assert!(html.contains("<pre>"), "{html}");
    assert!(!html.contains("<p>^ cap</p>"), "{html}");
}
