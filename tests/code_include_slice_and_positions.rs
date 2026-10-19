use carve::{
    expand_includes, parse_with_options, render_html, to_json, IncludeDenial, IncludeOptions,
    IncludeResolved, Options,
};
use serde_json::{json, Value};

fn run(directive: &str, file: &str, source: &str) -> (String, Value) {
    let resolver = |path: &str, _: &carve::IncludeContext<'_>| {
        (path == file)
            .then(|| IncludeResolved::from(source))
            .ok_or(IncludeDenial::NotFound)
    };
    let doc = parse_with_options(directive, &Options::default().with_positions(true));
    let result = expand_includes(
        doc,
        directive,
        &IncludeOptions::new().with_resolver(&resolver),
    );
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    (
        render_html(&result.doc).unwrap(),
        serde_json::from_str(&to_json(&result.doc)).unwrap(),
    )
}

fn pos(tree: &Value) -> &Value {
    &tree["children"][0]["pos"]
}
fn expected(file: &str, sl: usize, sc: usize, so: usize, el: usize, ec: usize, eo: usize) -> Value {
    json!({"file":file,"startLine":sl,"startColumn":sc,"startOffset":so,"endLine":el,"endColumn":ec,"endOffset":eo})
}

#[test]
fn code_payload_and_positions() {
    let cases = [
        ("s.js", "a\nbb\n", "{{ s.js @code:js }}", "<pre><code class=\"language-js\">a\nbb\n</code></pre>", (1,1,0,2,3,4)),
        ("t.txt", "x\n\n\n", "{{ t.txt @code }}", "<pre><code>x\n\n\n</code></pre>", (1,1,0,3,1,3)),
        ("t.txt", "x\n\n\n", "{{ t.txt @lines:1-3 @code }}", "<pre><code>x\n\n\n</code></pre>", (1,1,0,3,1,3)),
        ("t.txt", "x\n\n\n", "{{ t.txt @lines:2-2 @code }}", "<pre><code>\n</code></pre>", (2,1,2,2,1,2)),
        ("r.js", "// #region a\nx\n\n// #endregion a\n// #region b\n\n// #endregion b\n// #region c\n// #endregion c\n", "{{ r.js @region:a @code }}", "<pre><code>x\n\n</code></pre>", (2,1,13,3,1,15)),
        ("r.js", "// #region a\nx\n\n// #endregion a\n// #region b\n\n// #endregion b\n// #region c\n// #endregion c\n", "{{ r.js @region:b @code }}", "<pre><code>\n</code></pre>", (6,1,45,6,1,45)),
        ("r.js", "// #region a\nx\n\n// #endregion a\n// #region b\n\n// #endregion b\n// #region c\n// #endregion c\n", "{{ r.js @region:c @code }}", "<pre><code></code></pre>", (9,1,75,9,1,75)),
        ("c.js", "a\r\nbb\r\n", "{{ c.js @code }}", "<pre><code>a\nbb\n</code></pre>", (1,1,0,2,3,5)),
        ("cr.js", "a\rbb\r", "{{ cr.js @code }}", "<pre><code>a\nbb\n</code></pre>", (1,1,0,2,3,4)),
        ("bom.js", "\u{feff}a\nb\n", "{{ bom.js @code }}", "<pre><code>a\nb\n</code></pre>", (1,2,1,2,2,4)),
        ("bom.js", "\u{feff}a\nb\n", "{{ bom.js @lines:1-1 @code }}", "<pre><code>a\n</code></pre>", (1,2,1,1,3,2)),
        ("bom.js", "\u{feff}a\nb\n", "{{ bom.js @lines:2-2 @code }}", "<pre><code>b\n</code></pre>", (2,1,3,2,2,4)),
        ("empty.txt", "", "{{ empty.txt @code }}", "<pre><code></code></pre>", (1,1,0,1,1,0)),
        ("bom.txt", "\u{feff}", "{{ bom.txt @code }}", "<pre><code></code></pre>", (1,2,1,1,2,1)),
    ];
    for (file, source, directive, html, (sl, sc, so, el, ec, eo)) in cases {
        let (actual, tree) = run(directive, file, source);
        assert_eq!(actual, html, "{directive}");
        assert_eq!(
            pos(&tree),
            &expected(file, sl, sc, so, el, ec, eo),
            "{directive}"
        );
    }
}

#[test]
fn carve_crlf_slices_keep_original_offsets() {
    let source = "one\r\n\r\ntwo\r\n";
    let (html, tree) = run("{{ c.crv @lines:1-3 }}", "c.crv", source);
    assert_eq!(html, "<p>one</p>\n<p>two</p>");
    let two = tree["children"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node.to_string().contains("two"))
        .unwrap();
    assert_eq!(two["pos"], expected("c.crv", 3, 1, 7, 3, 4, 10));
    let region = "%% #region a\r\none\r\n\r\ntwo\r\n%% #endregion a\r\n";
    let (html, tree) = run("{{ c.crv @region:a }}", "c.crv", region);
    assert_eq!(html, "<p>one</p>\n<p>two</p>");
    let two = tree["children"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node.to_string().contains("two"))
        .unwrap();
    assert_eq!(two["pos"], expected("c.crv", 4, 1, 21, 4, 4, 24));
}

#[test]
fn attributed_code_include_table_keeps_file_span() {
    let (html, tree) = run(
        "{table}\n{{ data.csv @code:csv }}",
        "data.csv",
        "a,b\n1,2\n",
    );
    assert_eq!(html, "<table>\n  <thead>\n    <tr><th scope=\"col\">a</th><th scope=\"col\">b</th></tr>\n  </thead>\n  <tbody>\n    <tr><td>1</td><td>2</td></tr>\n  </tbody>\n</table>");
    assert_eq!(pos(&tree), &expected("data.csv", 1, 1, 0, 2, 4, 7));
}
