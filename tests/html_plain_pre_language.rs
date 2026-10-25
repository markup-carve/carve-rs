use carve::{html_to_ast, html_to_carve, HtmlImportOptions};
use serde_json::Value;
use std::time::Instant;

#[test]
fn sole_plain_pre_class_is_fallback_language() {
    let options = HtmlImportOptions::default();
    for html in [
        "<pre class=\"python\">x</pre>",
        "<pre class=\"python\"><code>x</code></pre>",
    ] {
        assert!(html_to_carve(html, &options)
            .unwrap()
            .value
            .contains("```python"));
        let ast = html_to_ast(html, &options).unwrap();
        let tree: Value = serde_json::from_str(&carve::to_json(&ast.value)).unwrap();
        assert_eq!(tree["children"][0]["lang"], "python");
    }
    assert!(html_to_carve(
        "<div class=\"highlight\"><pre class=\"python\">x</pre></div>",
        &options,
    )
    .unwrap()
    .value
    .contains("```python"));
    for (html, language) in [
        (
            "<pre class=\"python\"><code class=\"language-js\">x</code></pre>",
            Some("js"),
        ),
        (
            "<div class=\"highlight highlight-source-js\"><pre class=\"prettyprint\">x</pre></div>",
            Some("js"),
        ),
        ("<pre class=\"python extra\">x</pre>", None),
        ("<pre class=\"d\">x</pre>", None),
        ("<pre class=\"mermaid\">graph TD</pre>", None),
    ] {
        let source = html_to_carve(html, &options).unwrap().value;
        match language {
            Some(lang) => assert!(source.contains(&format!("```{lang}")), "{html}: {source}"),
            None => assert!(
                !source.contains("```python")
                    && !source.contains("```d\n")
                    && !source.contains("```mermaid"),
                "{html}: {source}"
            ),
        }
    }
    let structural = html_to_carve("<pre class=\"mermaid\">graph TD</pre>", &options)
        .unwrap()
        .value;
    assert!(structural.contains("{.mermaid}"), "{structural}");
}

#[test]
fn plain_pre_import_timing() {
    for count in [2_000, 8_000, 32_000] {
        let html = "<pre class=\"python\">x</pre>".repeat(count);
        let start = Instant::now();
        let ast = html_to_ast(&html, &HtmlImportOptions::default()).unwrap();
        assert_eq!(ast.value.children.len(), count);
        eprintln!(
            "plain pre import {count}: {:.3}s",
            start.elapsed().as_secs_f64()
        );
    }
}
