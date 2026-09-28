#[test]
fn fence_languages_survive_import() {
    for (source, expected) in [
        (
            "```&#99;\nx\n```",
            "<pre><code class=\"language-c\">x\n</code></pre>",
        ),
        (
            "```c\\#\nx\n```",
            "<pre><code class=\"language-c#\">x\n</code></pre>",
        ),
        ("```a b\nx\n```", "<pre><code>x\n</code></pre>"),
        (
            "``` f&ouml;&ouml;\nfoo\n```\n",
            "<pre><code>foo\n</code></pre>",
        ),
        ("````;\n````\n", "<pre><code>\n</code></pre>"),
        (
            "```=html\n<script>x</script>\n```",
            "<pre><code>&lt;script&gt;x&lt;/script&gt;\n</code></pre>",
        ),
        ("~~~a`b\nx\n~~~", "<pre><code>x\n</code></pre>"),
        ("```a\"b\n\"x\"\n```", "<pre><code>\"x\"\n</code></pre>"),
        (
            "```c++ title=x\nx\n```",
            "<pre><code class=\"language-c++\">x\n</code></pre>",
        ),
        (
            "- ```föö\n  x\n  ```",
            "<ul>\n  <li>\n    <pre><code>x\n</code></pre>\n  </li>\n</ul>",
        ),
        (
            "> ```föö\n> x\n> ```",
            "<blockquote>\n  <pre><code>x\n</code></pre>\n</blockquote>",
        ),
    ] {
        let converted = carve::markdown_to_carve(source);
        assert_eq!(carve::to_html(&converted).trim(), expected, "{source}");
    }
}
