//! A block opener at a hosted note's floor ends the `+` block, and a nested
//! note opened after an attached quote keeps its own lines (carve#5512).

fn html(source: &str) -> String {
    carve::to_html(source)
}

#[test]
fn fence_after_plus_paragraph_keeps_its_payload() {
    let out = html("- [^a]: +\n  p\n    ``` x\n    [^z]: code\n\nsee[^a]\n");
    assert!(
        out.contains("<pre><code class=\"language-x\">[^z]: code\n\n</code></pre>"),
        "{out}"
    );
    assert!(out.contains("<p>p<") || out.contains("<p>p</p>"), "{out}");
    assert!(!out.contains("<code> x"), "{out}");
}

#[test]
fn fence_after_plus_quote_keeps_its_payload() {
    let out = html("- [^a]: +\n  > q\n    ``` x\n    [^z]: code\n\nsee[^a]\n");
    assert!(out.contains("<blockquote><p>q</p></blockquote>"), "{out}");
    assert!(
        out.contains("<pre><code class=\"language-x\">[^z]: code\n\n</code></pre>"),
        "{out}"
    );
}

#[test]
fn heading_and_quote_after_plus_paragraph_are_blocks() {
    let out = html("- [^a]: +\n  p\n    # h\n\nsee[^a]\n");
    assert!(out.contains("<h1 id=\"h\">h</h1>"), "{out}");
    assert!(!out.contains("p\n# h"), "{out}");
    let out = html("- [^a]: +\n  p\n    > q\n\nsee[^a]\n");
    assert!(out.contains("<blockquote><p>q</p></blockquote>"), "{out}");
    assert!(!out.contains("&gt; q"), "{out}");
}

#[test]
fn nested_note_after_attached_quote_keeps_its_comment() {
    let out = html("[^n0]: +\n> q\n  [^n1]: nb\n    %%%\n  ind1\n\nsee[^n0][^n1]\n");
    assert!(
        out.contains("<blockquote><p>q</p></blockquote>\n      <p>ind1<a href=\"#fnref1\""),
        "{out}"
    );
    assert!(out.contains("<p>nb<a href=\"#fnref2\""), "{out}");
}
