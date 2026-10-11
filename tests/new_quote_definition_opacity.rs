use carve::{parse, render_html};

#[test]
fn a_fence_tests_paragraph_ownership_in_its_admitted_quote() {
    let cases = [
        ("new-quote-note", "p\n> ```\n> [^a]: note\n", "<p>p</p>\n<blockquote>\n  <pre><code>[^a]: note\n</code></pre>\n</blockquote>", 0),
        ("new-quote-link", "p\n> ```\n> [r]: /target\n", "<p>p</p>\n<blockquote>\n  <pre><code>[r]: /target\n</code></pre>\n</blockquote>", 0),
        ("new-quote-abbr", "p\n> ```\n> [@a]: expansion\n", "<p>p</p>\n<blockquote>\n  <pre><code>[@a]: expansion\n</code></pre>\n</blockquote>", 0),
        ("same-quote-paragraph", "> p\n> ```\n> [^a]: note\n", "<blockquote><p>p\n<code></code></p></blockquote>", 1),
        ("indented-new-quote", "p\n  > ```\n  > [^a]: note\n", "<p>p\n&gt; <code>\n&gt; [^a]: note</code></p>", 0),
        ("blank-before-new-quote", "p\n\n> ```\n> [^a]: note\n", "<p>p</p>\n<blockquote>\n  <pre><code>[^a]: note\n</code></pre>\n</blockquote>", 0),
        ("deeper-new-quote", "> p\n> > ```\n> > [^a]: note\n", "<blockquote>\n  <p>p</p>\n  <blockquote>\n    <pre><code>[^a]: note\n</code></pre>\n  </blockquote>\n</blockquote>", 0),
        ("tilde-new-quote", "p\n> ~~~\n> [^a]: note\n", "<p>p</p>\n<blockquote>\n  <pre><code>[^a]: note\n</code></pre>\n</blockquote>", 0),
    ];
    for (name, source, expected, notes) in cases {
        let doc = parse(source);
        assert_eq!(render_html(&doc).unwrap(), expected, "{name}");
        assert_eq!(doc.footnote_defs.len(), notes, "{name}");
    }
}

#[test]
fn definitions_after_an_unclosed_quote_fence_are_outside_its_payload() {
    for marker in ["```", "~~~"] {
        let source = format!("p\n> {marker}\n> [^inside]: payload\n\n[^outside]: body\n");
        let doc = parse(&source);
        assert!(!doc.footnote_defs.contains_key("inside"));
        assert!(doc.footnote_defs.contains_key("outside"));
        assert!(render_html(&doc).unwrap().contains("[^inside]: payload"));
    }
}
