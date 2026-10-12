use carve::{parse, render_html};

#[test]
fn a_fence_tests_paragraph_ownership_in_its_admitted_quote() {
    let cases = [
        ("new-quote-note", "p\n> ```\n> [^a]: note\n", "<p>p</p>\n<blockquote>\n  <pre><code>[^a]: note\n</code></pre>\n</blockquote>", 0),
        ("new-quote-link", "p\n> ```\n> [r]: /target\n", "<p>p</p>\n<blockquote>\n  <pre><code>[r]: /target\n</code></pre>\n</blockquote>", 0),
        ("new-quote-citation", "p\n> ```\n> [@a]: expansion\n", "<p>p</p>\n<blockquote>\n  <pre><code>[@a]: expansion\n</code></pre>\n</blockquote>", 0),
        ("same-quote-paragraph", "> p\n> ```\n> [^a]: note\n", "<blockquote><p>p\n<code></code></p></blockquote>", 1),
        ("indented-new-quote", "p\n  > ```\n  > [^a]: note\n", "<p>p\n&gt; <code>\n&gt; [^a]: note</code></p>", 0),
        ("blank-before-new-quote", "p\n\n> ```\n> [^a]: note\n", "<p>p</p>\n<blockquote>\n  <pre><code>[^a]: note\n</code></pre>\n</blockquote>", 0),
        ("deeper-new-quote", "> p\n> > ```\n> > [^a]: note\n", "<blockquote>\n  <p>p</p>\n  <blockquote>\n    <pre><code>[^a]: note\n</code></pre>\n  </blockquote>\n</blockquote>", 0),
        ("tilde-new-quote", "p\n> ~~~\n> [^a]: note\n", "<p>p</p>\n<blockquote>\n  <pre><code>[^a]: note\n</code></pre>\n</blockquote>", 0),
        ("review-paragraph-0", "> p\n>   # not a heading\n> ```\n> [^a]: note\n", "<blockquote><p>p\n# not a heading\n<code></code></p></blockquote>", 1),
        ("review-paragraph-1", "> p\n> ![a](b.png)\n> ```\n> [^a]: note\n", "<blockquote><p>p\n<img src=\"b.png\" alt=\"a\">\n<code></code></p></blockquote>", 1),
        ("review-paragraph-2", "> p\n> : x\n> ```\n> [^a]: note\n", "<blockquote><p>p\n: x\n<code></code></p></blockquote>", 1),
        ("review-paragraph-3", ":: t\n: a\n  more\n: > p\n  > ```\n  > [^a]: note\n", "<dl>\n  <dt>t</dt>\n  <dd>a\nmore</dd>\n  <dd>\n    <blockquote><p>p\n<code></code></p></blockquote>\n  </dd>\n</dl>", 1),
        ("review-marker-paragraph-0", "> p\n> - # h\n> ```\n> [^a]: note\n", "<blockquote><p>p\n- # h\n<code></code></p></blockquote>", 1),
        ("review-marker-paragraph-1", ":: t\n: d\n\n> p\n> : # x\n> ```\n> [^a]: note\n", "<dl>\n  <dt>t</dt>\n  <dd>d</dd>\n</dl>\n<blockquote><p>p\n: # x\n<code></code></p></blockquote>", 1),
        ("review-lazy-marker-paragraph", "> p\n- # h\n> ```\n> [^a]: note\n", "<blockquote><p>p\n- # h\n<code></code></p></blockquote>", 1),
        ("review-host-boundary-0", "> > p\n>   > # h\n> > ```\n> > [^a]: note\n", "<blockquote>\n  <blockquote><p>p\n&gt; # h\n<code></code></p></blockquote>\n</blockquote>", 1),
        ("review-host-boundary-1", "p\n> ```\n> [^a]: note\n\n:: u\n~~~\n[r]: /u\n\n[x][r]\n", "<p>p</p>\n<blockquote>\n  <pre><code>[^a]: note\n</code></pre>\n</blockquote>\n<dl>\n  <dt>u</dt>\n</dl>\n<pre><code>[r]: /u\n\n[x][r]\n</code></pre>", 0),
        ("review-quoted-payload-0", "> - a\n>     ```\n>   [r]: /u\n>     ```\n", "<blockquote>\n  <ul>\n    <li>a\n      <pre><code>[r]: /u\n</code></pre>\n    </li>\n  </ul>\n</blockquote>", 0),
        ("review-quoted-payload-1", "> :::note\n> :::\n> ```\n> [^a]: n\n", "<blockquote><p>:::note\n:::\n<code></code></p></blockquote>", 1),
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

#[test]
fn quote_host_transitions_keep_folding_and_definition_boundaries() {
    let cases = [
        ("quoted-marker-code-then-note", "> - ```\n>   code\n>   ```\n> [^a]: note\n", "<blockquote>\n  <ul>\n    <li>\n      <pre><code>code\n</code></pre>\n    </li>\n  </ul>\n</blockquote>"),
        ("blank-description-followed-term-code", ":: t\n\n: > p\n  > ```\n  > [^a]: note\n\n:: u\n~~~\n[r]: /u\n\n[x][r]\n", "<dl>\n  <dt>t</dt>\n  <dd>\n    <blockquote><p>p\n<code></code></p></blockquote>\n  </dd>\n  <dt>u</dt>\n</dl>\n<pre><code>[r]: /u\n\n[x][r]\n</code></pre>"),
        ("indented-fence-text", ">  ~~~\n> ~~~\n> [^a]: n\n", "<blockquote><p>~~~\n~~~</p></blockquote>"),
        ("indented-list-quote", "- a\n   > b\n   ~~~\n[r]: /u\n", "<ul>\n  <li>a\n    <blockquote><p>b\n~~~</p></blockquote>\n  </li>\n</ul>"),
        ("indented-inner-quote", "> p\n>   > b\n> ```\n> [^a]: n\n", "<blockquote><p>p\n&gt; b\n<code></code></p></blockquote>"),
        ("lazy-after-code-payload", "> ```\n> x\nlazy\n> ~~~\n> [^a]: note\n", "<blockquote>\n  <pre><code>x\n</code></pre>\n</blockquote>\n<p>lazy</p>\n<blockquote>\n  <pre><code>[^a]: note\n</code></pre>\n</blockquote>"),
        ("lazy-after-closed-code-payload", "> ```\n> x\n> ```\nlazy\n> ~~~\n> [^a]: note\n", "<blockquote>\n  <pre><code>x\n</code></pre>\n</blockquote>\n<p>lazy</p>\n<blockquote>\n  <pre><code>[^a]: note\n</code></pre>\n</blockquote>"),
        ("lazy-after-comment-payload", "> %%%\n> x\n> %%%\nlazy\n> ```\n> [^a]: note\n", "<blockquote>\n\n</blockquote>\n<p>lazy</p>\n<blockquote>\n  <pre><code>[^a]: note\n</code></pre>\n</blockquote>"),
        ("lazy-list-host-term", "- > p\n  lazy\n  > ```\n  > [^a]: note\n\n:: t\n", "<ul>\n  <li>\n    <blockquote><p>p\nlazy\n<code></code></p></blockquote>\n  </li>\n</ul>\n<dl>\n  <dt>t</dt>\n</dl>"),
        ("lazy-after-code", "> ```\nlazy\n> ~~~\n> [^a]: note\n", "<blockquote>\n  <pre><code></code></pre>\n</blockquote>\n<p>lazy</p>\n<blockquote>\n  <pre><code>[^a]: note\n</code></pre>\n</blockquote>"),
        ("lazy-after-hr", "> ---\nlazy\n> ```\n> [^a]: note\n", "<blockquote>\n  <hr>\n</blockquote>\n<p>lazy</p>\n<blockquote>\n  <pre><code>[^a]: note\n</code></pre>\n</blockquote>"),
        ("lazy-description-host", ":: t\n\n: > p\nlazy\n  > ```\n  > [^a]: note\n", "<dl>\n  <dt>t</dt>\n  <dd>\n    <blockquote><p>p\nlazy\n<code></code></p></blockquote>\n  </dd>\n</dl>"),
        ("lazy-after-inline-fence", "> p\n> ```\nlazy\n> ~~~\n> [^a]: note\n", "<blockquote><p>p\n<code>\nlazy\n~~~</code></p></blockquote>"),
        ("lazy-quote", "> p\nlazy\n> ```\n> [^a]: note\n", "<blockquote><p>p\nlazy\n<code></code></p></blockquote>"),
        ("lazy-deep-quote", "> > p\n> lazy\n> > ```\n> > [^a]: note\n", "<blockquote>\n  <blockquote><p>p\nlazy\n<code></code></p></blockquote>\n</blockquote>"),
        ("lazy-blank-boundary", "> p\nlazy\n\n> ```\n> [^a]: note\n", "<blockquote><p>p\nlazy</p></blockquote>\n<blockquote>\n  <pre><code>[^a]: note\n</code></pre>\n</blockquote>"),
        ("lazy-heading-boundary", "> p\nlazy\n# h\n> ```\n> [^a]: note\n", "<blockquote><p>p\nlazy</p></blockquote>\n<section id=\"h\">\n  <h1>h</h1>\n  <blockquote>\n    <pre><code>[^a]: note\n</code></pre>\n  </blockquote>\n</section>"),
        ("lazy-comment-boundary", "> p\nlazy\n%% comment\n> ```\n> [^a]: note\n", "<blockquote><p>p\nlazy</p></blockquote>\n<blockquote>\n  <pre><code>[^a]: note\n</code></pre>\n</blockquote>"),
        ("lazy-list-boundary", "> p\nlazy\n- next\n> ```\n> [^a]: note\n", "<blockquote><p>p\nlazy\n- next\n<code></code></p></blockquote>"),
        ("review-description-floor", ":: t\n\n: > p\n  > ```\n  > [^a]: note\n", "<dl>\n  <dt>t</dt>\n  <dd>\n    <blockquote><p>p\n<code></code></p></blockquote>\n  </dd>\n</dl>"),
        ("exit-note", "> p\n```\n\n[^a]: note\n", "<blockquote><p>p\n<code></code></p></blockquote>"),
        ("exit-link", "> p\n```\n\n[r]: /u\n", "<blockquote><p>p\n<code></code></p></blockquote>"),
        ("exit-abbr", "> p\n```\n\n*[a]: expansion\n", "<blockquote><p>p\n<code></code></p></blockquote>"),
        ("nested-exit", "> > p\n> ```\n> [^a]: note\n", "<blockquote>\n  <blockquote><p>p\n<code></code></p></blockquote>\n</blockquote>"),
        ("closed-list-quote", "> - ```\n> [r]: /u\n>   ```\n", "<blockquote>\n  <ul>\n    <li>\n      <pre><code></code></pre>\n    </li>\n  </ul>\n  <p><code></code></p>\n</blockquote>"),
        ("term-blank-quote", ":: t\n\n: > ```\n> [^a]: note\n\n[^outside]: body\n", "<dl>\n  <dt>t</dt>\n  <dd>\n    <blockquote>\n      <pre><code></code></pre>\n    </blockquote>\n  </dd>\n</dl>\n<blockquote>\n\n</blockquote>"),
        ("same-list-host", "- > p\n  > ```\n  > [^a]: note\n", "<ul>\n  <li>\n    <blockquote><p>p\n<code></code></p></blockquote>\n  </li>\n</ul>"),
        ("blank-list-host", "- > p\n\n  > ```\n  > [^a]: note\n", "<ul>\n  <li>\n    <blockquote><p>p</p></blockquote>\n    <blockquote>\n      <pre><code>[^a]: note\n</code></pre>\n    </blockquote>\n  </li>\n</ul>"),
        ("quote-root-link-after", "p\n> ```\n> [^a]: note\n\n[r]: /u\n\n[x][r]\n", "<p>p</p>\n<blockquote>\n  <pre><code>[^a]: note\n</code></pre>\n</blockquote>\n<p><a href=\"/u\">x</a></p>"),
        ("closed-list-quote-owned", "> - ```\n>   [r]: /u\n>   ```\n", "<blockquote>\n  <ul>\n    <li>\n      <pre><code>[r]: /u\n</code></pre>\n    </li>\n  </ul>\n</blockquote>"),
        ("description-blank-owned-note", ":: t\n\n: > ```\n   > [^a]: note\n\n[^outside]: body\n", "<dl>\n  <dt>t</dt>\n  <dd>\n    <blockquote>\n      <pre><code>[^a]: note\n</code></pre>\n    </blockquote>\n  </dd>\n</dl>"),
        ("description-blank-paragraph", ":: t\n\n: > p\n   > ```\n   > [^a]: note\n", "<dl>\n  <dt>t</dt>\n  <dd>\n    <blockquote><p>p\n&gt; <code>\n&gt; [^a]: note</code></p></blockquote>\n  </dd>\n</dl>"),
        ("description-blank-owned-link", ":: t\n\n: > ```\n   > [r]: /u\n\n[x][r]\n", "<dl>\n  <dt>t</dt>\n  <dd>\n    <blockquote>\n      <pre><code>[r]: /u\n</code></pre>\n    </blockquote>\n  </dd>\n</dl>\n<p>[x][r]</p>"),
        ("new-quote-true-abbr", "p\n> ```\n> *[a]: expansion\n", "<p>p</p>\n<blockquote>\n  <pre><code>*[a]: expansion\n</code></pre>\n</blockquote>"),
    ];
    for (name, source, expected) in cases {
        let doc = parse(source);
        assert_eq!(render_html(&doc).unwrap(), expected, "{name}");
    }
}
