use carve::{parse, render_html};

#[test]
#[ignore = "rule A ledger: carve-rs#3524"]
fn a_fence_tests_paragraph_ownership_in_its_admitted_quote() {
    let cases = [
        ("new-quote-note", "p\n> ```\n> [^a]: note\n", "<p>p</p>\n<blockquote>\n  <pre><code>[^a]: note\n</code></pre>\n</blockquote>", 0),
        ("new-quote-link", "p\n> ```\n> [r]: /target\n", "<p>p</p>\n<blockquote>\n  <pre><code>[r]: /target\n</code></pre>\n</blockquote>", 0),
        ("new-quote-citation", "p\n> ```\n> [@a]: expansion\n", "<p>p</p>\n<blockquote>\n  <pre><code>[@a]: expansion\n</code></pre>\n</blockquote>", 0),
        ("same-quote-paragraph", "> p\n> ```\n> [^a]: note\n", "<blockquote><p>p\n<code></code></p></blockquote>", 0),
        ("indented-new-quote", "p\n  > ```\n  > [^a]: note\n", "<p>p\n&gt; <code>\n&gt; [^a]: note</code></p>", 0),
        ("blank-before-new-quote", "p\n\n> ```\n> [^a]: note\n", "<p>p</p>\n<blockquote>\n  <pre><code>[^a]: note\n</code></pre>\n</blockquote>", 0),
        ("deeper-new-quote", "> p\n> > ```\n> > [^a]: note\n", "<blockquote>\n  <p>p</p>\n  <blockquote>\n    <pre><code>[^a]: note\n</code></pre>\n  </blockquote>\n</blockquote>", 0),
        ("tilde-new-quote", "p\n> ~~~\n> [^a]: note\n", "<p>p</p>\n<blockquote>\n  <pre><code>[^a]: note\n</code></pre>\n</blockquote>", 0),
        ("review-paragraph-0", "> p\n>   # not a heading\n> ```\n> [^a]: note\n", "<blockquote><p>p\n# not a heading\n<code></code></p></blockquote>", 0),
        ("review-paragraph-1", "> p\n> ![a](b.png)\n> ```\n> [^a]: note\n", "<blockquote><p>p\n<img src=\"b.png\" alt=\"a\">\n<code></code></p></blockquote>", 0),
        ("review-paragraph-2", "> p\n> : x\n> ```\n> [^a]: note\n", "<blockquote><p>p\n: x\n<code></code></p></blockquote>", 0),
        ("review-paragraph-3", ":: t\n: a\n  more\n: > p\n  > ```\n  > [^a]: note\n", "<dl>\n  <dt>t</dt>\n  <dd>a\nmore</dd>\n  <dd>\n    <blockquote><p>p\n<code></code></p></blockquote>\n  </dd>\n</dl>", 0),
        ("review-marker-paragraph-0", "> p\n> - # h\n> ```\n> [^a]: note\n", "<blockquote><p>p\n- # h\n<code></code></p></blockquote>", 0),
        ("review-marker-paragraph-1", ":: t\n: d\n\n> p\n> : # x\n> ```\n> [^a]: note\n", "<dl>\n  <dt>t</dt>\n  <dd>d</dd>\n</dl>\n<blockquote><p>p\n: # x\n<code></code></p></blockquote>", 0),
        ("review-lazy-marker-paragraph", "> p\n- # h\n> ```\n> [^a]: note\n", "<blockquote><p>p\n- # h\n<code></code></p></blockquote>", 0),
        ("review-host-boundary-0", "> > p\n>   > # h\n> > ```\n> > [^a]: note\n", "<blockquote>\n  <blockquote><p>p\n&gt; # h\n<code></code></p></blockquote>\n</blockquote>", 0),
        ("review-host-boundary-1", "p\n> ```\n> [^a]: note\n\n:: u\n~~~\n[r]: /u\n\n[x][r]\n", "<p>p</p>\n<blockquote>\n  <pre><code>[^a]: note\n</code></pre>\n</blockquote>\n<dl>\n  <dt>u</dt>\n</dl>\n<pre><code>[r]: /u\n\n[x][r]\n</code></pre>", 0),
        ("review-quoted-payload-0", "> - a\n>     ```\n>   [r]: /u\n>     ```\n", "<blockquote>\n  <ul>\n    <li>a\n      <pre><code>[r]: /u\n</code></pre>\n    </li>\n  </ul>\n</blockquote>", 0),
        ("review-quoted-payload-1", "> :::note\n> :::\n> ```\n> [^a]: n\n", "<blockquote><p>:::note\n:::\n<code></code></p></blockquote>", 0),
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
#[ignore = "rule A ledger: carve-rs#3524"]
fn quote_host_transitions_keep_folding_and_definition_boundaries() {
    let cases = [
        ("quoted-marker-code-then-note", "> - ```\n>   code\n>   ```\n> [^a]: note\n", "<blockquote>\n  <ul>\n    <li>\n      <pre><code>code\n</code></pre>\n    </li>\n  </ul>\n</blockquote>"),
        ("blank-description-followed-term-code", ":: t\n\n: > p\n  > ```\n  > [^a]: note\n\n:: u\n~~~\n[r]: /u\n\n[x][r]\n", "<dl>\n  <dt>t</dt>\n  <dd>\n    <blockquote><p>p\n<code></code></p></blockquote>\n  </dd>\n  <dt>u</dt>\n</dl>\n<pre><code>[r]: /u\n\n[x][r]\n</code></pre>"),
        ("indented-fence-text", ">  ~~~\n> ~~~\n> [^a]: n\n", "<blockquote>\n  <pre><code></code></pre>\n</blockquote>"),
        ("indented-list-quote", "- a\n   > b\n   ~~~\n[r]: /u\n", "<ul>\n  <li>a\n    <blockquote><p>b\n~~~</p></blockquote>\n  </li>\n</ul>"),
        ("indented-inner-quote", "> p\n>   > b\n> ```\n> [^a]: n\n", "<blockquote>\n  <p>p</p>\n  <blockquote><p>b\n<code></code></p></blockquote>\n</blockquote>"),
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
        ("closed-list-quote", "> - ```\n> [r]: /u\n>   ```\n", "<blockquote>\n  <ul>\n    <li>\n      <pre><code></code></pre>\n    </li>\n  </ul>\n  <pre><code></code></pre>\n</blockquote>"),
        ("term-blank-quote", ":: t\n\n: > ```\n> [^a]: note\n\n[^outside]: body\n", "<dl>\n  <dt>t</dt>\n  <dd>\n    <blockquote>\n      <pre><code></code></pre>\n    </blockquote>\n  </dd>\n</dl>\n<blockquote>\n\n</blockquote>"),
        ("same-list-host", "- > p\n  > ```\n  > [^a]: note\n", "<ul>\n  <li>\n    <blockquote><p>p\n<code></code></p></blockquote>\n  </li>\n</ul>"),
        ("blank-list-host", "- > p\n\n  > ```\n  > [^a]: note\n", "<ul>\n  <li>\n    <blockquote><p>p</p></blockquote>\n    <blockquote>\n      <pre><code>[^a]: note\n</code></pre>\n    </blockquote>\n  </li>\n</ul>"),
        ("quote-root-link-after", "p\n> ```\n> [^a]: note\n\n[r]: /u\n\n[x][r]\n", "<p>p</p>\n<blockquote>\n  <pre><code>[^a]: note\n</code></pre>\n</blockquote>\n<p><a href=\"/u\">x</a></p>"),
        ("closed-list-quote-owned", "> - ```\n>   [r]: /u\n>   ```\n", "<blockquote>\n  <ul>\n    <li>\n      <pre><code>[r]: /u\n</code></pre>\n    </li>\n  </ul>\n</blockquote>"),
        ("description-blank-owned-note", ":: t\n\n: > ```\n   > [^a]: note\n\n[^outside]: body\n", "<dl>\n  <dt>t</dt>\n  <dd>\n    <blockquote>\n      <pre><code>%%\u{e006}       \n</code></pre>\n    </blockquote>\n  </dd>\n</dl>"),
        ("description-blank-paragraph", ":: t\n\n: > p\n   > ```\n   > [^a]: note\n", "<dl>\n  <dt>t</dt>\n  <dd>\n    <blockquote><p>p\n&gt; <code>\n&gt; [^a]: note</code></p></blockquote>\n  </dd>\n</dl>"),
        ("description-blank-owned-link", ":: t\n\n: > ```\n   > [r]: /u\n\n[x][r]\n", "<dl>\n  <dt>t</dt>\n  <dd>\n    <blockquote>\n      <pre><code>[r]: /u\n</code></pre>\n    </blockquote>\n  </dd>\n</dl>\n<p>[x][r]</p>"),
        ("new-quote-true-abbr", "p\n> ```\n> *[a]: expansion\n", "<p>p</p>\n<blockquote>\n  <pre><code>*[a]: expansion\n</code></pre>\n</blockquote>"),
    ];
    for (name, source, expected) in cases {
        let doc = parse(source);
        assert_eq!(render_html(&doc).unwrap(), expected, "{name}");
    }
}

#[test]
fn a_folded_inline_fence_keeps_payload_literal_and_releases_later_definitions() {
    let cases = [
        ("p\n- ```\n  x\n```\n\n[r]: /u\n\nsee [r] and [^a]\n\n:: t\n", "<p>p\n- <code>\nx</code></p>\n<pre><code>\n[r]: /u\n\nsee [r] and [^a]\n\n:: t\n</code></pre>", 0),
        ("p\n- ```\n  [r]: /u\n```\n\nsee [r] and [^a]\n\n:: t\n", "<p>p\n- <code></code></p>\n<pre><code>\nsee [r] and [^a]\n\n:: t\n</code></pre>", 0),
        ("p\n- ```\n  x\n```\n\n[^a]: note\n\nsee [r] and [^a]\n\n:: t\n", "<p>p\n- <code>\nx</code></p>\n<pre><code>\n[^a]: note\n\nsee [r] and [^a]\n\n:: t\n</code></pre>", 0),
        ("p\n- ```\n  [^a]: note\n```\n\nsee [r] and [^a]\n\n:: t\n", "<p>p\n- <code></code></p>\n<pre><code>\nsee [r] and [^a]\n\n:: t\n</code></pre>",1),
        ("> p\n> - ```\n>   x\n> ```\n> \n> [r]: /u\n> \n> see [r] and [^a]\n> \n> :: t\n", "<blockquote>\n  <p>p\n- <code>\nx</code></p>\n  <pre><code>\n[r]: /u\n\nsee [r] and [^a]\n\n:: t\n</code></pre>\n</blockquote>", 0),
        ("> p\n> - ```\n>   [r]: /u\n> ```\n> \n> see [r] and [^a]\n> \n> :: t\n", "<blockquote>\n  <p>p\n- <code></code></p>\n  <pre><code>\nsee [r] and [^a]\n\n:: t\n</code></pre>\n</blockquote>", 0),
        ("> p\n> - ```\n>   x\n> ```\n> \n> [^a]: note\n> \n> see [r] and [^a]\n> \n> :: t\n", "<blockquote>\n  <p>p\n- <code>\nx</code></p>\n  <pre><code>\n[^a]: note\n\nsee [r] and [^a]\n\n:: t\n</code></pre>\n</blockquote>", 0),
        ("> p\n> - ```\n>   [^a]: note\n> ```\n> \n> see [r] and [^a]\n> \n> :: t\n", "<blockquote>\n  <p>p\n- <code></code></p>\n  <pre><code>\nsee [r] and [^a]\n\n:: t\n</code></pre>\n</blockquote>",1),
    ];
    for (source, expected, notes) in cases {
        let doc = parse(source);
        assert_eq!(render_html(&doc).unwrap(), expected, "{source:?}");
        assert_eq!(doc.footnote_defs.len(), notes, "{source:?}");
        let without_term = source.replace("> :: t\n", "").replace(":: t\n", "");
        let without_term_html = expected
            .replace("\n  <dl>\n    <dt>t</dt>\n  </dl>", "")
            .replace("\n<dl>\n  <dt>t</dt>\n</dl>", "")
            .replace(":: t\n</code>", "</code>");
        let doc = parse(&without_term);
        assert_eq!(
            render_html(&doc).unwrap(),
            without_term_html,
            "{without_term:?}"
        );
        assert_eq!(doc.footnote_defs.len(), notes, "{without_term:?}");
    }
}

#[test]
fn sibling_items_and_blanks_keep_their_real_fence_boundaries() {
    let cases = [
        ("> - a\n>   prose\n> - ```\n>   [r]: /u\n>   ```\n> \n> see [r]\n", "<blockquote>\n  <ul>\n    <li>a\nprose</li>\n    <li>\n      <pre><code>[r]: /u\n</code></pre>\n    </li>\n  </ul>\n  <p>see [r]</p>\n</blockquote>"),
        ("> p\n> - ```\n>   x\n> \n> ```\n> [r]: /u\n> \n> see [r]\n", "<blockquote>\n  <p>p\n- <code>\nx</code></p>\n  <pre><code>[r]: /u\n\nsee [r]\n</code></pre>\n</blockquote>"),
        ("> > - a\n> >   prose\n> > - ```\n> >   [r]: /u\n> >   ```\n> > \n> > see [r]\n", "<blockquote>\n  <blockquote>\n    <ul>\n      <li>a\nprose</li>\n      <li>\n        <pre><code>[r]: /u\n</code></pre>\n      </li>\n    </ul>\n    <p>see [r]</p>\n  </blockquote>\n</blockquote>"),
        ("> > p\n> > - ```\n> >   x\n> > \n> > ```\n> > [r]: /u\n> > \n> > see [r]\n", "<blockquote>\n  <blockquote>\n    <p>p\n- <code>\nx</code></p>\n    <pre><code>[r]: /u\n\nsee [r]\n</code></pre>\n  </blockquote>\n</blockquote>"),
    ];
    for (source, expected) in cases {
        let doc = parse(source);
        assert_eq!(render_html(&doc).unwrap(), expected, "{source:?}");
        assert!(doc.footnote_defs.is_empty(), "{source:?}");
    }
}

#[test]
fn inline_closers_restore_paragraphs_without_demoting_real_siblings() {
    let cases = [
        ("> p\n> - ```\n>   x\n> ```\n> ~~~\n> \n> [r]: /u\n> \n> see [r]\n", "<blockquote>\n  <p>p\n- <code>\nx</code></p>\n  <pre><code>~~~\n\n[r]: /u\n\nsee [r]\n</code></pre>\n</blockquote>"),
        ("> - a\n> - ```\n>   x\n> \n>   ~~~\n>   ```\n>   :: t\n>   ```\n>   [r]: /u\n> \n> [x][r]\n", "<blockquote>\n  <ul>\n    <li>a</li>\n    <li>\n      <pre><code>x\n\n~~~\n</code></pre>\n      <dl>\n        <dt>t</dt>\n      </dl>\n      <pre><code>[r]: /u\n\n</code></pre>\n    </li>\n  </ul>\n  <p>[x][r]</p>\n</blockquote>"),
    ];
    for (source, expected) in cases {
        assert_eq!(render_html(&parse(source)).unwrap(), expected, "{source:?}");
    }
}

#[test]
fn folded_inline_fences_admit_lazy_text_and_keep_block_exits() {
    let cases = [
        ("> p\n> - ```\n>   x\nlazy\n> ```\n> [^a]: note\n", "<blockquote>\n  <p>p\n- <code>\nx\nlazy</code></p>\n  <pre><code>[^a]: note\n</code></pre>\n</blockquote>"),
        ("> p\n> - ```\n>   x\n# heading\n> ```\n> [^a]: note\n", "<blockquote><p>p\n- <code>\nx</code></p></blockquote>\n<section id=\"heading\">\n  <h1>heading</h1>\n  <blockquote>\n    <pre><code>[^a]: note\n</code></pre>\n  </blockquote>\n</section>"),
        ("> p\n> - ```\n>   x\n\n> ```\n> [^a]: note\n", "<blockquote><p>p\n- <code>\nx</code></p></blockquote>\n<blockquote>\n  <pre><code>[^a]: note\n</code></pre>\n</blockquote>"),
        ("> p\n> - ```\n>   x\n[r]: /u\n> ```\n> [^a]: note\n", "<blockquote><p>p\n- <code>\nx</code></p></blockquote>\n<blockquote>\n  <pre><code>[^a]: note\n</code></pre>\n</blockquote>"),
        ("> > p\n> > - ```\n> >   x\nlazy\n> > ```\n> > [^a]: note\n", "<blockquote>\n  <blockquote>\n    <p>p\n- <code>\nx\nlazy</code></p>\n    <pre><code>[^a]: note\n</code></pre>\n  </blockquote>\n</blockquote>"),
        ("> > p\n> > - ```\n> >   x\n# heading\n> > ```\n> > [^a]: note\n", "<blockquote>\n  <blockquote><p>p\n- <code>\nx</code></p></blockquote>\n</blockquote>\n<section id=\"heading\">\n  <h1>heading</h1>\n  <blockquote>\n    <blockquote>\n      <pre><code>[^a]: note\n</code></pre>\n    </blockquote>\n  </blockquote>\n</section>"),
        ("> > p\n> > - ```\n> >   x\n\n> > ```\n> > [^a]: note\n", "<blockquote>\n  <blockquote><p>p\n- <code>\nx</code></p></blockquote>\n</blockquote>\n<blockquote>\n  <blockquote>\n    <pre><code>[^a]: note\n</code></pre>\n  </blockquote>\n</blockquote>"),
        ("> > p\n> > - ```\n> >   x\n[r]: /u\n> > ```\n> > [^a]: note\n", "<blockquote>\n  <blockquote><p>p\n- <code>\nx</code></p></blockquote>\n</blockquote>\n<blockquote>\n  <blockquote>\n    <pre><code>[^a]: note\n</code></pre>\n  </blockquote>\n</blockquote>"),
    ];
    for (source, expected) in cases {
        assert_eq!(render_html(&parse(source)).unwrap(), expected, "{source:?}");
    }
}

#[test]
fn partial_quote_prefixes_preserve_definition_and_blank_exits() {
    let cases = [
        ("> > p\n> > - ```\n> >   x\n> [r]: /u\n> > ```\n> > [^a]: note\n\n[x][r]\n", "<blockquote>\n  <blockquote><p>p\n- <code>\nx</code></p></blockquote>\n  <blockquote>\n    <pre><code>[^a]: note\n</code></pre>\n  </blockquote>\n</blockquote>\n<p><a href=\"/u\">x</a></p>"),
        ("> > p\n> > - ```\n> >   x\n> [r]: /u\n> > ```\n> > [r]: /u\n\n[x][r]\n", "<blockquote>\n  <blockquote><p>p\n- <code>\nx</code></p></blockquote>\n  <blockquote>\n    <pre><code>[r]: /u\n</code></pre>\n  </blockquote>\n</blockquote>\n<p><a href=\"/u\">x</a></p>"),
        ("> > p\n> > - ```\n> >   x\n>\n> > ```\n> > [^a]: note\n\n[x][r]\n", "<blockquote>\n  <blockquote><p>p\n- <code>\nx</code></p></blockquote>\n  <blockquote>\n    <pre><code>[^a]: note\n</code></pre>\n  </blockquote>\n</blockquote>\n<p>[x][r]</p>"),
        ("> > p\n> > - ```\n> >   x\n>\n> > ```\n> > [r]: /u\n\n[x][r]\n", "<blockquote>\n  <blockquote><p>p\n- <code>\nx</code></p></blockquote>\n  <blockquote>\n    <pre><code>[r]: /u\n</code></pre>\n  </blockquote>\n</blockquote>\n<p>[x][r]</p>"),
        ("> > p\n> > - ```\n> >   x\n> lazy\n> > ```\n> > [^a]: note\n\n[x][r]\n", "<blockquote>\n  <blockquote>\n    <p>p\n- <code>\nx\nlazy</code></p>\n    <pre><code>[^a]: note\n</code></pre>\n  </blockquote>\n</blockquote>\n<p>[x][r]</p>"),
        ("> > p\n> > - ```\n> >   x\n> lazy\n> > ```\n> > [r]: /u\n\n[x][r]\n", "<blockquote>\n  <blockquote>\n    <p>p\n- <code>\nx\nlazy</code></p>\n    <pre><code>[r]: /u\n</code></pre>\n  </blockquote>\n</blockquote>\n<p>[x][r]</p>"),
    ];
    for (source, expected) in cases {
        assert_eq!(render_html(&parse(source)).unwrap(), expected, "{source:?}");
    }
}

#[test]
fn folded_inline_fences_keep_neighboring_block_boundaries() {
    let cases = [
        ("> p\n> - ```\n>   x\n- y\n> ```\n> [r]: /u\n\n[x][r]\n", "<blockquote>\n  <p>p\n- <code>\nx\n- y</code></p>\n  <pre><code>[r]: /u\n</code></pre>\n</blockquote>\n<p>[x][r]</p>"),
        ("> p\n> - ```\n>   x\n:: t\n> ```\n> [r]: /u\n\n[x][r]\n", "<blockquote><p>p\n- <code>\nx</code></p></blockquote>\n<dl>\n  <dt>t</dt>\n</dl>\n<blockquote>\n  <pre><code>[r]: /u\n</code></pre>\n</blockquote>\n<p>[x][r]</p>"),
        ("> p\n> - ```\n>   x\n: body\n> ```\n> [r]: /u\n\n[x][r]\n", "<blockquote>\n  <p>p\n- <code>\nx\n: body</code></p>\n  <pre><code>[r]: /u\n</code></pre>\n</blockquote>\n<p>[x][r]</p>"),
        ("> p\n> - ```\n>   x\n%% comment\n> ```\n> [r]: /u\n\n[x][r]\n", "<blockquote><p>p\n- <code>\nx</code></p></blockquote>\n<blockquote>\n  <pre><code>[r]: /u\n</code></pre>\n</blockquote>\n<p>[x][r]</p>"),
        ("> p\n> - ```\n>   x\n%%%\n> ```\n> [r]: /u\n\n[x][r]\n", "<blockquote><p>p\n- <code>\nx</code></p></blockquote>\n<blockquote>\n  <pre><code>[r]: /u\n</code></pre>\n</blockquote>\n<p>[x][r]</p>"),
        ("> p\n> - ```\n>   x\n:::bad\n> ```\n> [r]: /u\n\n[x][r]\n", "<blockquote>\n  <p>p\n- <code>\nx\n:::bad</code></p>\n  <pre><code>[r]: /u\n</code></pre>\n</blockquote>\n<p>[x][r]</p>"),
        ("> p\n> - ```\n>   x\n:::\n> ```\n> [r]: /u\n\n[x][r]\n", "<blockquote><p>p\n- <code>\nx</code></p></blockquote>\n<div>\n  <blockquote>\n    <pre><code>[r]: /u\n</code></pre>\n  </blockquote>\n  <p>[x][r]</p>\n</div>"),
        ("> p\n> - ```\n>   x\n> lazy\n> ```\n> [r]: /u\n\n[x][r]\n", "<blockquote>\n  <p>p\n- <code>\nx\nlazy</code></p>\n  <pre><code>[r]: /u\n</code></pre>\n</blockquote>\n<p>[x][r]</p>"),
        ("> p\n> - ```\n>   x\n> # heading\n> ```\n> [r]: /u\n\n[x][r]\n", "<blockquote>\n  <p>p\n- <code>\nx</code></p>\n  <h1 id=\"heading\">heading</h1>\n  <pre><code>[r]: /u\n</code></pre>\n</blockquote>\n<p>[x][r]</p>"),
        ("> p\n> - ```\n>   x\n   lazy\n> ```\n> [r]: /u\n\n[x][r]\n", "<blockquote>\n  <p>p\n- <code>\nx\nlazy</code></p>\n  <pre><code>[r]: /u\n</code></pre>\n</blockquote>\n<p>[x][r]</p>"),
        ("> > p\n> > - ```\n> >   x\n- y\n> > ```\n> > [r]: /u\n\n[x][r]\n", "<blockquote>\n  <blockquote>\n    <p>p\n- <code>\nx\n- y</code></p>\n    <pre><code>[r]: /u\n</code></pre>\n  </blockquote>\n</blockquote>\n<p>[x][r]</p>"),
        ("> > p\n> > - ```\n> >   x\n:: t\n> > ```\n> > [r]: /u\n\n[x][r]\n", "<blockquote>\n  <blockquote><p>p\n- <code>\nx</code></p></blockquote>\n</blockquote>\n<dl>\n  <dt>t</dt>\n</dl>\n<blockquote>\n  <blockquote>\n    <pre><code>[r]: /u\n</code></pre>\n  </blockquote>\n</blockquote>\n<p>[x][r]</p>"),
        ("> > p\n> > - ```\n> >   x\n: body\n> > ```\n> > [r]: /u\n\n[x][r]\n", "<blockquote>\n  <blockquote>\n    <p>p\n- <code>\nx\n: body</code></p>\n    <pre><code>[r]: /u\n</code></pre>\n  </blockquote>\n</blockquote>\n<p>[x][r]</p>"),
        ("> > p\n> > - ```\n> >   x\n%% comment\n> > ```\n> > [r]: /u\n\n[x][r]\n", "<blockquote>\n  <blockquote><p>p\n- <code>\nx</code></p></blockquote>\n</blockquote>\n<blockquote>\n  <blockquote>\n    <pre><code>[r]: /u\n</code></pre>\n  </blockquote>\n</blockquote>\n<p>[x][r]</p>"),
        ("> > p\n> > - ```\n> >   x\n%%%\n> > ```\n> > [r]: /u\n\n[x][r]\n", "<blockquote>\n  <blockquote><p>p\n- <code>\nx</code></p></blockquote>\n</blockquote>\n<blockquote>\n  <blockquote>\n    <pre><code>[r]: /u\n</code></pre>\n  </blockquote>\n</blockquote>\n<p>[x][r]</p>"),
        ("> > p\n> > - ```\n> >   x\n:::bad\n> > ```\n> > [r]: /u\n\n[x][r]\n", "<blockquote>\n  <blockquote>\n    <p>p\n- <code>\nx\n:::bad</code></p>\n    <pre><code>[r]: /u\n</code></pre>\n  </blockquote>\n</blockquote>\n<p>[x][r]</p>"),
        ("> > p\n> > - ```\n> >   x\n:::\n> > ```\n> > [r]: /u\n\n[x][r]\n", "<blockquote>\n  <blockquote><p>p\n- <code>\nx</code></p></blockquote>\n</blockquote>\n<div>\n  <blockquote>\n    <blockquote>\n      <pre><code>[r]: /u\n</code></pre>\n    </blockquote>\n  </blockquote>\n  <p>[x][r]</p>\n</div>"),
        ("> > p\n> > - ```\n> >   x\n> lazy\n> > ```\n> > [r]: /u\n\n[x][r]\n", "<blockquote>\n  <blockquote>\n    <p>p\n- <code>\nx\nlazy</code></p>\n    <pre><code>[r]: /u\n</code></pre>\n  </blockquote>\n</blockquote>\n<p>[x][r]</p>"),
        ("> > p\n> > - ```\n> >   x\n> # heading\n> > ```\n> > [r]: /u\n\n[x][r]\n", "<blockquote>\n  <blockquote><p>p\n- <code>\nx</code></p></blockquote>\n  <h1 id=\"heading\">heading</h1>\n  <blockquote>\n    <pre><code>[r]: /u\n</code></pre>\n  </blockquote>\n</blockquote>\n<p>[x][r]</p>"),
        ("> > p\n> > - ```\n> >   x\n   lazy\n> > ```\n> > [r]: /u\n\n[x][r]\n", "<blockquote>\n  <blockquote>\n    <p>p\n- <code>\nx\nlazy</code></p>\n    <pre><code>[r]: /u\n</code></pre>\n  </blockquote>\n</blockquote>\n<p>[x][r]</p>"),
    ];
    for (source, expected) in cases {
        assert_eq!(render_html(&parse(source)).unwrap(), expected, "{source:?}");
    }
}

#[test]
#[ignore = "rule A ledger: carve-rs#3524"]
fn quoted_item_witness_and_owned_absorption_match_native() {
    let cases = [
        (
            "item-witness-1-0",
            "> - a\n> - ```\n>   x\n> \n>   ~~~\n>   ```\n>   [r]: /u\n\n[x][r]\n",
            "<blockquote>\n  <ul>\n    <li>a</li>\n    <li>\n      <pre><code>x\n\n~~~\n</code></pre>\n    </li>\n  </ul>\n</blockquote>\n<p><a href=\"/u\">x</a></p>",
        ),
        (
            "item-witness-1-1",
            "> - a\n>  - ```\n>    x\n> \n>    ~~~\n>    ```\n>    [r]: /u\n\n[x][r]\n",
            "<blockquote>\n  <ul>\n    <li>a\n- <code>\nx</code>\n      <pre><code>```\n[r]: /u\n</code></pre>\n    </li>\n  </ul>\n</blockquote>\n<p>[x][r]</p>",
        ),
        (
            "item-witness-1-2",
            "> - a\n>   - ```\n>     x\n> \n>     ~~~\n>     ```\n>     [r]: /u\n\n[x][r]\n",
            "<blockquote>\n  <ul>\n    <li>a\n      <ul>\n        <li>\n          <pre><code>x\n\n~~~\n</code></pre>\n        </li>\n      </ul>\n    </li>\n  </ul>\n</blockquote>\n<p><a href=\"/u\">x</a></p>",
        ),
        (
            "item-witness-1-3",
            "> - a\n>    - ```\n>      x\n> \n>      ~~~\n>      ```\n>      [r]: /u\n\n[x][r]\n",
            "<blockquote>\n  <ul>\n    <li>a\n      <ul>\n        <li>\n          <pre><code>x\n\n~~~\n</code></pre>\n        </li>\n      </ul>\n    </li>\n  </ul>\n</blockquote>\n<p><a href=\"/u\">x</a></p>",
        ),
        (
            "item-witness-1-4",
            "> - a\n>     - ```\n>       x\n> \n>       ~~~\n>       ```\n>       [r]: /u\n\n[x][r]\n",
            "<blockquote>\n  <ul>\n    <li>a\n      <ul>\n        <li>\n          <pre><code>x\n\n~~~\n</code></pre>\n        </li>\n      </ul>\n    </li>\n  </ul>\n</blockquote>\n<p><a href=\"/u\">x</a></p>",
        ),
        (
            "item-witness-1-5",
            "> p\n> - ```\n>   x\n> \n>   ~~~\n>   ```\n>   [r]: /u\n\n[x][r]\n",
            "<blockquote>\n  <p>p\n- <code>\nx</code></p>\n  <p>~~~\n<code>\n[r]: /u</code></p>\n</blockquote>\n<p>[x][r]</p>",
        ),
        (
            "item-witness-1-6",
            "> p\n>  - ```\n>    x\n> \n>    ~~~\n>    ```\n>    [r]: /u\n\n[x][r]\n",
            "<blockquote>\n  <p>p\n- <code>\nx</code></p>\n  <p>~~~\n<code>\n[r]: /u</code></p>\n</blockquote>\n<p>[x][r]</p>",
        ),
        (
            "item-witness-1-7",
            "> p\n>   - ```\n>     x\n> \n>     ~~~\n>     ```\n>     [r]: /u\n\n[x][r]\n",
            "<blockquote>\n  <p>p\n- <code>\nx</code></p>\n  <p>~~~\n<code>\n[r]: /u</code></p>\n</blockquote>\n<p>[x][r]</p>",
        ),
        (
            "item-witness-1-8",
            "> p\n>    - ```\n>      x\n> \n>      ~~~\n>      ```\n>      [r]: /u\n\n[x][r]\n",
            "<blockquote>\n  <p>p\n- <code>\nx</code></p>\n  <p>~~~\n<code>\n[r]: /u</code></p>\n</blockquote>\n<p>[x][r]</p>",
        ),
        (
            "item-witness-1-9",
            "> p\n>     - ```\n>       x\n> \n>       ~~~\n>       ```\n>       [r]: /u\n\n[x][r]\n",
            "<blockquote>\n  <p>p\n- <code>\nx</code></p>\n  <p>~~~\n<code>\n[r]: /u</code></p>\n</blockquote>\n<p>[x][r]</p>",
        ),
        (
            "item-witness-1-10",
            "> p\n> - a\n> - ```\n>   x\n> \n>   ~~~\n>   ```\n>   [r]: /u\n\n[x][r]\n",
            "<blockquote>\n  <p>p\n- a\n- <code>\nx</code></p>\n  <p>~~~\n<code>\n[r]: /u</code></p>\n</blockquote>\n<p>[x][r]</p>",
        ),
        (
            "item-witness-1-11",
            "> p\n> - a\n>  - ```\n>    x\n> \n>    ~~~\n>    ```\n>    [r]: /u\n\n[x][r]\n",
            "<blockquote>\n  <p>p\n- a\n- <code>\nx</code></p>\n  <p>~~~\n<code>\n[r]: /u</code></p>\n</blockquote>\n<p>[x][r]</p>",
        ),
        (
            "item-witness-1-12",
            "> p\n> - a\n>   - ```\n>     x\n> \n>     ~~~\n>     ```\n>     [r]: /u\n\n[x][r]\n",
            "<blockquote>\n  <p>p\n- a\n- <code>\nx</code></p>\n  <p>~~~\n<code>\n[r]: /u</code></p>\n</blockquote>\n<p>[x][r]</p>",
        ),
        (
            "item-witness-1-13",
            "> p\n> - a\n>    - ```\n>      x\n> \n>      ~~~\n>      ```\n>      [r]: /u\n\n[x][r]\n",
            "<blockquote>\n  <p>p\n- a\n- <code>\nx</code></p>\n  <p>~~~\n<code>\n[r]: /u</code></p>\n</blockquote>\n<p>[x][r]</p>",
        ),
        (
            "item-witness-1-14",
            "> p\n> - a\n>     - ```\n>       x\n> \n>       ~~~\n>       ```\n>       [r]: /u\n\n[x][r]\n",
            "<blockquote>\n  <p>p\n- a\n- <code>\nx</code></p>\n  <p>~~~\n<code>\n[r]: /u</code></p>\n</blockquote>\n<p>[x][r]</p>",
        ),
        (
            "item-witness-1-15",
            "> - a\n> lazy\n> - ```\n>   x\n> \n>   ~~~\n>   ```\n>   [r]: /u\n\n[x][r]\n",
            "<blockquote>\n  <ul>\n    <li>a\nlazy</li>\n    <li>\n      <pre><code>x\n\n~~~\n</code></pre>\n    </li>\n  </ul>\n</blockquote>\n<p><a href=\"/u\">x</a></p>",
        ),
        (
            "item-witness-1-16",
            "> - a\n> lazy\n>  - ```\n>    x\n> \n>    ~~~\n>    ```\n>    [r]: /u\n\n[x][r]\n",
            "<blockquote>\n  <ul>\n    <li>a\nlazy\n- <code>\nx</code>\n      <pre><code>```\n[r]: /u\n</code></pre>\n    </li>\n  </ul>\n</blockquote>\n<p>[x][r]</p>",
        ),
        (
            "item-witness-1-17",
            "> - a\n> lazy\n>   - ```\n>     x\n> \n>     ~~~\n>     ```\n>     [r]: /u\n\n[x][r]\n",
            "<blockquote>\n  <ul>\n    <li>a\nlazy\n      <ul>\n        <li>\n          <pre><code>x\n\n~~~\n</code></pre>\n        </li>\n      </ul>\n    </li>\n  </ul>\n</blockquote>\n<p><a href=\"/u\">x</a></p>",
        ),
        (
            "item-witness-1-18",
            "> - a\n> lazy\n>    - ```\n>      x\n> \n>      ~~~\n>      ```\n>      [r]: /u\n\n[x][r]\n",
            "<blockquote>\n  <ul>\n    <li>a\nlazy\n      <ul>\n        <li>\n          <pre><code>x\n\n~~~\n</code></pre>\n        </li>\n      </ul>\n    </li>\n  </ul>\n</blockquote>\n<p><a href=\"/u\">x</a></p>",
        ),
        (
            "item-witness-1-19",
            "> - a\n> lazy\n>     - ```\n>       x\n> \n>       ~~~\n>       ```\n>       [r]: /u\n\n[x][r]\n",
            "<blockquote>\n  <ul>\n    <li>a\nlazy\n      <ul>\n        <li>\n          <pre><code>x\n\n~~~\n</code></pre>\n        </li>\n      </ul>\n    </li>\n  </ul>\n</blockquote>\n<p><a href=\"/u\">x</a></p>",
        ),
        (
            "owned-absorption-1-true",
            "> p\n> - ```\n>   x\n> :::bad\n> :::\n> ```\n> [r]: /u\n\n[x][r]\n",
            "<blockquote><p>p\n- <code>\nx\n:::bad\n:::\n</code></p></blockquote>\n<p><a href=\"/u\">x</a></p>",
        ),
        (
            "owned-absorption-1-false",
            "> p\n> - ```\n>   x\n:::bad\n:::\n> ```\n> [r]: /u\n\n[x][r]\n",
            "<blockquote><p>p\n- <code>\nx\n:::bad</code></p></blockquote>\n<div>\n  <blockquote>\n    <pre><code>[r]: /u\n</code></pre>\n  </blockquote>\n  <p>[x][r]</p>\n</div>",
        ),
        (
            "item-witness-2-22",
            "> > - a\n> > - ```\n> >   x\n> > \n> >   ~~~\n> >   ```\n> >   [r]: /u\n\n[x][r]\n",
            "<blockquote>\n  <blockquote>\n    <ul>\n      <li>a</li>\n      <li>\n        <pre><code>x\n\n~~~\n</code></pre>\n      </li>\n    </ul>\n  </blockquote>\n</blockquote>\n<p><a href=\"/u\">x</a></p>",
        ),
        (
            "item-witness-2-23",
            "> > - a\n> >  - ```\n> >    x\n> > \n> >    ~~~\n> >    ```\n> >    [r]: /u\n\n[x][r]\n",
            "<blockquote>\n  <blockquote>\n    <ul>\n      <li>a\n- <code>\nx</code>\n        <pre><code>```\n[r]: /u\n</code></pre>\n      </li>\n    </ul>\n  </blockquote>\n</blockquote>\n<p>[x][r]</p>",
        ),
        (
            "item-witness-2-24",
            "> > - a\n> >   - ```\n> >     x\n> > \n> >     ~~~\n> >     ```\n> >     [r]: /u\n\n[x][r]\n",
            "<blockquote>\n  <blockquote>\n    <ul>\n      <li>a\n        <ul>\n          <li>\n            <pre><code>x\n\n~~~\n</code></pre>\n          </li>\n        </ul>\n      </li>\n    </ul>\n  </blockquote>\n</blockquote>\n<p><a href=\"/u\">x</a></p>",
        ),
        (
            "item-witness-2-25",
            "> > - a\n> >    - ```\n> >      x\n> > \n> >      ~~~\n> >      ```\n> >      [r]: /u\n\n[x][r]\n",
            "<blockquote>\n  <blockquote>\n    <ul>\n      <li>a\n        <ul>\n          <li>\n            <pre><code>x\n\n~~~\n</code></pre>\n          </li>\n        </ul>\n      </li>\n    </ul>\n  </blockquote>\n</blockquote>\n<p><a href=\"/u\">x</a></p>",
        ),
        (
            "item-witness-2-26",
            "> > - a\n> >     - ```\n> >       x\n> > \n> >       ~~~\n> >       ```\n> >       [r]: /u\n\n[x][r]\n",
            "<blockquote>\n  <blockquote>\n    <ul>\n      <li>a\n        <ul>\n          <li>\n            <pre><code>x\n\n~~~\n</code></pre>\n          </li>\n        </ul>\n      </li>\n    </ul>\n  </blockquote>\n</blockquote>\n<p><a href=\"/u\">x</a></p>",
        ),
        (
            "item-witness-2-27",
            "> > p\n> > - ```\n> >   x\n> > \n> >   ~~~\n> >   ```\n> >   [r]: /u\n\n[x][r]\n",
            "<blockquote>\n  <blockquote>\n    <p>p\n- <code>\nx</code></p>\n    <p>~~~\n<code>\n[r]: /u</code></p>\n  </blockquote>\n</blockquote>\n<p>[x][r]</p>",
        ),
        (
            "item-witness-2-28",
            "> > p\n> >  - ```\n> >    x\n> > \n> >    ~~~\n> >    ```\n> >    [r]: /u\n\n[x][r]\n",
            "<blockquote>\n  <blockquote>\n    <p>p\n- <code>\nx</code></p>\n    <p>~~~\n<code>\n[r]: /u</code></p>\n  </blockquote>\n</blockquote>\n<p>[x][r]</p>",
        ),
        (
            "item-witness-2-29",
            "> > p\n> >   - ```\n> >     x\n> > \n> >     ~~~\n> >     ```\n> >     [r]: /u\n\n[x][r]\n",
            "<blockquote>\n  <blockquote>\n    <p>p\n- <code>\nx</code></p>\n    <p>~~~\n<code>\n[r]: /u</code></p>\n  </blockquote>\n</blockquote>\n<p>[x][r]</p>",
        ),
        (
            "item-witness-2-30",
            "> > p\n> >    - ```\n> >      x\n> > \n> >      ~~~\n> >      ```\n> >      [r]: /u\n\n[x][r]\n",
            "<blockquote>\n  <blockquote>\n    <p>p\n- <code>\nx</code></p>\n    <p>~~~\n<code>\n[r]: /u</code></p>\n  </blockquote>\n</blockquote>\n<p>[x][r]</p>",
        ),
        (
            "item-witness-2-31",
            "> > p\n> >     - ```\n> >       x\n> > \n> >       ~~~\n> >       ```\n> >       [r]: /u\n\n[x][r]\n",
            "<blockquote>\n  <blockquote>\n    <p>p\n- <code>\nx</code></p>\n    <p>~~~\n<code>\n[r]: /u</code></p>\n  </blockquote>\n</blockquote>\n<p>[x][r]</p>",
        ),
        (
            "item-witness-2-32",
            "> > p\n> > - a\n> > - ```\n> >   x\n> > \n> >   ~~~\n> >   ```\n> >   [r]: /u\n\n[x][r]\n",
            "<blockquote>\n  <blockquote>\n    <p>p\n- a\n- <code>\nx</code></p>\n    <p>~~~\n<code>\n[r]: /u</code></p>\n  </blockquote>\n</blockquote>\n<p>[x][r]</p>",
        ),
        (
            "item-witness-2-33",
            "> > p\n> > - a\n> >  - ```\n> >    x\n> > \n> >    ~~~\n> >    ```\n> >    [r]: /u\n\n[x][r]\n",
            "<blockquote>\n  <blockquote>\n    <p>p\n- a\n- <code>\nx</code></p>\n    <p>~~~\n<code>\n[r]: /u</code></p>\n  </blockquote>\n</blockquote>\n<p>[x][r]</p>",
        ),
        (
            "item-witness-2-34",
            "> > p\n> > - a\n> >   - ```\n> >     x\n> > \n> >     ~~~\n> >     ```\n> >     [r]: /u\n\n[x][r]\n",
            "<blockquote>\n  <blockquote>\n    <p>p\n- a\n- <code>\nx</code></p>\n    <p>~~~\n<code>\n[r]: /u</code></p>\n  </blockquote>\n</blockquote>\n<p>[x][r]</p>",
        ),
        (
            "item-witness-2-35",
            "> > p\n> > - a\n> >    - ```\n> >      x\n> > \n> >      ~~~\n> >      ```\n> >      [r]: /u\n\n[x][r]\n",
            "<blockquote>\n  <blockquote>\n    <p>p\n- a\n- <code>\nx</code></p>\n    <p>~~~\n<code>\n[r]: /u</code></p>\n  </blockquote>\n</blockquote>\n<p>[x][r]</p>",
        ),
        (
            "item-witness-2-36",
            "> > p\n> > - a\n> >     - ```\n> >       x\n> > \n> >       ~~~\n> >       ```\n> >       [r]: /u\n\n[x][r]\n",
            "<blockquote>\n  <blockquote>\n    <p>p\n- a\n- <code>\nx</code></p>\n    <p>~~~\n<code>\n[r]: /u</code></p>\n  </blockquote>\n</blockquote>\n<p>[x][r]</p>",
        ),
        (
            "item-witness-2-37",
            "> > - a\n> > lazy\n> > - ```\n> >   x\n> > \n> >   ~~~\n> >   ```\n> >   [r]: /u\n\n[x][r]\n",
            "<blockquote>\n  <blockquote>\n    <ul>\n      <li>a\nlazy</li>\n      <li>\n        <pre><code>x\n\n~~~\n</code></pre>\n      </li>\n    </ul>\n  </blockquote>\n</blockquote>\n<p><a href=\"/u\">x</a></p>",
        ),
        (
            "item-witness-2-38",
            "> > - a\n> > lazy\n> >  - ```\n> >    x\n> > \n> >    ~~~\n> >    ```\n> >    [r]: /u\n\n[x][r]\n",
            "<blockquote>\n  <blockquote>\n    <ul>\n      <li>a\nlazy\n- <code>\nx</code>\n        <pre><code>```\n[r]: /u\n</code></pre>\n      </li>\n    </ul>\n  </blockquote>\n</blockquote>\n<p>[x][r]</p>",
        ),
        (
            "item-witness-2-39",
            "> > - a\n> > lazy\n> >   - ```\n> >     x\n> > \n> >     ~~~\n> >     ```\n> >     [r]: /u\n\n[x][r]\n",
            "<blockquote>\n  <blockquote>\n    <ul>\n      <li>a\nlazy\n        <ul>\n          <li>\n            <pre><code>x\n\n~~~\n</code></pre>\n          </li>\n        </ul>\n      </li>\n    </ul>\n  </blockquote>\n</blockquote>\n<p><a href=\"/u\">x</a></p>",
        ),
        (
            "item-witness-2-40",
            "> > - a\n> > lazy\n> >    - ```\n> >      x\n> > \n> >      ~~~\n> >      ```\n> >      [r]: /u\n\n[x][r]\n",
            "<blockquote>\n  <blockquote>\n    <ul>\n      <li>a\nlazy\n        <ul>\n          <li>\n            <pre><code>x\n\n~~~\n</code></pre>\n          </li>\n        </ul>\n      </li>\n    </ul>\n  </blockquote>\n</blockquote>\n<p><a href=\"/u\">x</a></p>",
        ),
        (
            "item-witness-2-41",
            "> > - a\n> > lazy\n> >     - ```\n> >       x\n> > \n> >       ~~~\n> >       ```\n> >       [r]: /u\n\n[x][r]\n",
            "<blockquote>\n  <blockquote>\n    <ul>\n      <li>a\nlazy\n        <ul>\n          <li>\n            <pre><code>x\n\n~~~\n</code></pre>\n          </li>\n        </ul>\n      </li>\n    </ul>\n  </blockquote>\n</blockquote>\n<p><a href=\"/u\">x</a></p>",
        ),
        (
            "owned-absorption-2-true",
            "> > p\n> > - ```\n> >   x\n> > :::bad\n> > :::\n> > ```\n> > [r]: /u\n\n[x][r]\n",
            "<blockquote>\n  <blockquote><p>p\n- <code>\nx\n:::bad\n:::\n</code></p></blockquote>\n</blockquote>\n<p><a href=\"/u\">x</a></p>",
        ),
        (
            "owned-absorption-2-false",
            "> > p\n> > - ```\n> >   x\n:::bad\n:::\n> > ```\n> > [r]: /u\n\n[x][r]\n",
            "<blockquote>\n  <blockquote><p>p\n- <code>\nx\n:::bad</code></p></blockquote>\n</blockquote>\n<div>\n  <blockquote>\n    <blockquote>\n      <pre><code>[r]: /u\n</code></pre>\n    </blockquote>\n  </blockquote>\n  <p>[x][r]</p>\n</div>",
        ),
    ];
    for (name, source, expected) in cases {
        assert_eq!(
            render_html(&parse(source)).unwrap().trim(),
            expected,
            "{name}"
        );
    }
}

#[test]
#[ignore = "rule A ledger: carve-rs#3524"]
fn nested_item_paragraphs_refresh_their_actual_marker_witness() {
    let cases = [
("nested-item-1-0","> - a\n>   - b\n> - ```\n>   x\n> \n>   ~~~\n>   ```\n>   [r]: /u\n\n[x][r]\n","<blockquote>\n  <ul>\n    <li>a\n      <ul>\n        <li>b</li>\n      </ul>\n    </li>\n    <li>\n      <pre><code>x\n\n~~~\n</code></pre>\n    </li>\n  </ul>\n</blockquote>\n<p><a href=\"/u\">x</a></p>"),
("nested-item-1-1","> - a\n>   - b\n>  - ```\n>    x\n> \n>    ~~~\n>    ```\n>    [r]: /u\n\n[x][r]\n","<blockquote>\n  <ul>\n    <li>a\n      <ul>\n        <li>b\n- <code>\nx</code></li>\n      </ul>\n      <pre><code>```\n[r]: /u\n</code></pre>\n    </li>\n  </ul>\n</blockquote>\n<p>[x][r]</p>"),
("nested-item-1-2","> - a\n>   - b\n>   - ```\n>     x\n> \n>     ~~~\n>     ```\n>     [r]: /u\n\n[x][r]\n","<blockquote>\n  <ul>\n    <li>a\n      <ul>\n        <li>b</li>\n        <li>\n          <pre><code>x\n\n~~~\n</code></pre>\n        </li>\n      </ul>\n    </li>\n  </ul>\n</blockquote>\n<p><a href=\"/u\">x</a></p>"),
("nested-item-1-3","> - a\n>   - b\n>    - ```\n>      x\n> \n>      ~~~\n>      ```\n>      [r]: /u\n\n[x][r]\n","<blockquote>\n  <ul>\n    <li>a\n      <ul>\n        <li>b\n- <code>\nx</code>\n          <pre><code>```\n[r]: /u\n</code></pre>\n        </li>\n      </ul>\n    </li>\n  </ul>\n</blockquote>\n<p>[x][r]</p>"),
("nested-item-1-4","> - a\n>   - b\n>     - ```\n>       x\n> \n>       ~~~\n>       ```\n>       [r]: /u\n\n[x][r]\n","<blockquote>\n  <ul>\n    <li>a\n      <ul>\n        <li>b\n          <ul>\n            <li>\n              <pre><code>x\n\n~~~\n</code></pre>\n            </li>\n          </ul>\n        </li>\n      </ul>\n    </li>\n  </ul>\n</blockquote>\n<p><a href=\"/u\">x</a></p>"),
("nested-item-1-5","> - a\n>   - b\n>      - ```\n>        x\n> \n>        ~~~\n>        ```\n>        [r]: /u\n\n[x][r]\n","<blockquote>\n  <ul>\n    <li>a\n      <ul>\n        <li>b\n          <ul>\n            <li>\n              <pre><code>x\n\n~~~\n</code></pre>\n            </li>\n          </ul>\n        </li>\n      </ul>\n    </li>\n  </ul>\n</blockquote>\n<p><a href=\"/u\">x</a></p>"),
("nested-item-1-6","> - a\n>   - b\n>       - ```\n>         x\n> \n>         ~~~\n>         ```\n>         [r]: /u\n\n[x][r]\n","<blockquote>\n  <ul>\n    <li>a\n      <ul>\n        <li>b\n          <ul>\n            <li>\n              <pre><code>x\n\n~~~\n</code></pre>\n            </li>\n          </ul>\n        </li>\n      </ul>\n    </li>\n  </ul>\n</blockquote>\n<p><a href=\"/u\">x</a></p>"),
("nested-item-2-0","> > - a\n> >   - b\n> > - ```\n> >   x\n> > \n> >   ~~~\n> >   ```\n> >   [r]: /u\n\n[x][r]\n","<blockquote>\n  <blockquote>\n    <ul>\n      <li>a\n        <ul>\n          <li>b</li>\n        </ul>\n      </li>\n      <li>\n        <pre><code>x\n\n~~~\n</code></pre>\n      </li>\n    </ul>\n  </blockquote>\n</blockquote>\n<p><a href=\"/u\">x</a></p>"),
("nested-item-2-1","> > - a\n> >   - b\n> >  - ```\n> >    x\n> > \n> >    ~~~\n> >    ```\n> >    [r]: /u\n\n[x][r]\n","<blockquote>\n  <blockquote>\n    <ul>\n      <li>a\n        <ul>\n          <li>b\n- <code>\nx</code></li>\n        </ul>\n        <pre><code>```\n[r]: /u\n</code></pre>\n      </li>\n    </ul>\n  </blockquote>\n</blockquote>\n<p>[x][r]</p>"),
("nested-item-2-2","> > - a\n> >   - b\n> >   - ```\n> >     x\n> > \n> >     ~~~\n> >     ```\n> >     [r]: /u\n\n[x][r]\n","<blockquote>\n  <blockquote>\n    <ul>\n      <li>a\n        <ul>\n          <li>b</li>\n          <li>\n            <pre><code>x\n\n~~~\n</code></pre>\n          </li>\n        </ul>\n      </li>\n    </ul>\n  </blockquote>\n</blockquote>\n<p><a href=\"/u\">x</a></p>"),
("nested-item-2-3","> > - a\n> >   - b\n> >    - ```\n> >      x\n> > \n> >      ~~~\n> >      ```\n> >      [r]: /u\n\n[x][r]\n","<blockquote>\n  <blockquote>\n    <ul>\n      <li>a\n        <ul>\n          <li>b\n- <code>\nx</code>\n            <pre><code>```\n[r]: /u\n</code></pre>\n          </li>\n        </ul>\n      </li>\n    </ul>\n  </blockquote>\n</blockquote>\n<p>[x][r]</p>"),
("nested-item-2-4","> > - a\n> >   - b\n> >     - ```\n> >       x\n> > \n> >       ~~~\n> >       ```\n> >       [r]: /u\n\n[x][r]\n","<blockquote>\n  <blockquote>\n    <ul>\n      <li>a\n        <ul>\n          <li>b\n            <ul>\n              <li>\n                <pre><code>x\n\n~~~\n</code></pre>\n              </li>\n            </ul>\n          </li>\n        </ul>\n      </li>\n    </ul>\n  </blockquote>\n</blockquote>\n<p><a href=\"/u\">x</a></p>"),
("nested-item-2-5","> > - a\n> >   - b\n> >      - ```\n> >        x\n> > \n> >        ~~~\n> >        ```\n> >        [r]: /u\n\n[x][r]\n","<blockquote>\n  <blockquote>\n    <ul>\n      <li>a\n        <ul>\n          <li>b\n            <ul>\n              <li>\n                <pre><code>x\n\n~~~\n</code></pre>\n              </li>\n            </ul>\n          </li>\n        </ul>\n      </li>\n    </ul>\n  </blockquote>\n</blockquote>\n<p><a href=\"/u\">x</a></p>"),
("nested-item-2-6","> > - a\n> >   - b\n> >       - ```\n> >         x\n> > \n> >         ~~~\n> >         ```\n> >         [r]: /u\n\n[x][r]\n","<blockquote>\n  <blockquote>\n    <ul>\n      <li>a\n        <ul>\n          <li>b\n            <ul>\n              <li>\n                <pre><code>x\n\n~~~\n</code></pre>\n              </li>\n            </ul>\n          </li>\n        </ul>\n      </li>\n    </ul>\n  </blockquote>\n</blockquote>\n<p><a href=\"/u\">x</a></p>"),
];
    for (name, source, expected) in cases {
        assert_eq!(
            render_html(&parse(source)).unwrap().trim(),
            expected,
            "{name}"
        );
    }
}

#[test]
#[ignore = "rule A ledger: carve-rs#3524"]
fn lazy_item_markers_preserve_the_owned_paragraph_witness() {
    let cases = [
("lazy-witness-0","> - a\n- b\n> - ```\n>   x\n> \n>   ~~~\n>   ```\n>   [r]: /u\n\n[x][r]\n","<blockquote>\n  <ul>\n    <li>a\n- b</li>\n    <li>\n      <pre><code>x\n\n~~~\n</code></pre>\n    </li>\n  </ul>\n</blockquote>\n<p><a href=\"/u\">x</a></p>"),
("lazy-witness-1","> - a\n- b\n>  - ```\n>    x\n> \n>    ~~~\n>    ```\n>    [r]: /u\n\n[x][r]\n","<blockquote>\n  <ul>\n    <li>a\n- b\n- <code>\nx</code>\n      <pre><code>```\n[r]: /u\n</code></pre>\n    </li>\n  </ul>\n</blockquote>\n<p>[x][r]</p>"),
("lazy-witness-2","> - a\n- b\n>   - ```\n>     x\n> \n>     ~~~\n>     ```\n>     [r]: /u\n\n[x][r]\n","<blockquote>\n  <ul>\n    <li>a\n- b\n      <ul>\n        <li>\n          <pre><code>x\n\n~~~\n</code></pre>\n        </li>\n      </ul>\n    </li>\n  </ul>\n</blockquote>\n<p><a href=\"/u\">x</a></p>"),
("lazy-witness-3","> - a\n- b\n>    - ```\n>      x\n> \n>      ~~~\n>      ```\n>      [r]: /u\n\n[x][r]\n","<blockquote>\n  <ul>\n    <li>a\n- b\n      <ul>\n        <li>\n          <pre><code>x\n\n~~~\n</code></pre>\n        </li>\n      </ul>\n    </li>\n  </ul>\n</blockquote>\n<p><a href=\"/u\">x</a></p>"),
("lazy-witness-4","> > - a\n> - b\n> > - ```\n> >   x\n> > \n> >   ~~~\n> >   ```\n> >   [r]: /u\n\n[x][r]\n","<blockquote>\n  <blockquote>\n    <ul>\n      <li>a\n- b</li>\n      <li>\n        <pre><code>x\n\n~~~\n</code></pre>\n      </li>\n    </ul>\n  </blockquote>\n</blockquote>\n<p><a href=\"/u\">x</a></p>"),
("lazy-witness-5","> > - a\n> - b\n> >  - ```\n> >    x\n> > \n> >    ~~~\n> >    ```\n> >    [r]: /u\n\n[x][r]\n","<blockquote>\n  <blockquote>\n    <ul>\n      <li>a\n- b\n- <code>\nx</code>\n        <pre><code>```\n[r]: /u\n</code></pre>\n      </li>\n    </ul>\n  </blockquote>\n</blockquote>\n<p>[x][r]</p>"),
("lazy-witness-6","> > - a\n> - b\n> >   - ```\n> >     x\n> > \n> >     ~~~\n> >     ```\n> >     [r]: /u\n\n[x][r]\n","<blockquote>\n  <blockquote>\n    <ul>\n      <li>a\n- b\n        <ul>\n          <li>\n            <pre><code>x\n\n~~~\n</code></pre>\n          </li>\n        </ul>\n      </li>\n    </ul>\n  </blockquote>\n</blockquote>\n<p><a href=\"/u\">x</a></p>"),
("lazy-witness-7","> > - a\n> - b\n> >    - ```\n> >      x\n> > \n> >      ~~~\n> >      ```\n> >      [r]: /u\n\n[x][r]\n","<blockquote>\n  <blockquote>\n    <ul>\n      <li>a\n- b\n        <ul>\n          <li>\n            <pre><code>x\n\n~~~\n</code></pre>\n          </li>\n        </ul>\n      </li>\n    </ul>\n  </blockquote>\n</blockquote>\n<p><a href=\"/u\">x</a></p>"),
];
    for (name, source, expected) in cases {
        assert_eq!(
            render_html(&parse(source)).unwrap().trim(),
            expected,
            "{name}"
        );
    }
}

#[test]
fn folded_markers_preserve_frames_and_real_quote_exits() {
    let cases = [
("frame-preservation-0",":: term\n> ```\nlazy\n> - x\n> ```\n> [r]: /t\n","<dl>\n  <dt>term</dt>\n</dl>\n<blockquote>\n  <pre><code></code></pre>\n</blockquote>\n<p>lazy</p>\n<blockquote>\n  <ul>\n    <li>x</li>\n  </ul>\n  <pre><code>[r]: /t\n</code></pre>\n</blockquote>"),
("frame-preservation-2","> > - a\n> - b\n> - ```\n> [r]: /t\n> ```\n","<blockquote>\n  <blockquote>\n    <ul>\n      <li>a\n- b\n- <code></code></li>\n    </ul>\n  </blockquote>\n  <pre><code></code></pre>\n</blockquote>"),
("frame-preservation-3",">   - a\n> - ```\n> [r]: /t\n> ```\n","<blockquote>\n  <ul>\n    <li>a</li>\n    <li>\n      <pre><code></code></pre>\n    </li>\n  </ul>\n  <pre><code></code></pre>\n</blockquote>"),
("frame-preservation-4","> p\n> ```\n> ```a\n> [r]: /t\n","<blockquote>\n  <p>p</p>\n  <pre><code>```a\n[r]: /t\n</code></pre>\n</blockquote>"),
];
    for (name, source, expected) in cases {
        assert_eq!(
            render_html(&parse(source)).unwrap().trim(),
            expected,
            "{name}"
        );
    }
}

#[test]
#[ignore = "rule A ledger: carve-rs#3524"]
fn folded_marker_definition_collection_preserves_preceding_items() {
    let source = "> - a\n>  - x\n>   - b\n> - c\n>  - ```\n> [r]: /t\n> ```\n\n[x][r]\n";
    let html = render_html(&parse(source)).unwrap();
    assert!(html.starts_with("<blockquote>\n  <ul>\n    <li>a\n- x\n      <ul>\n        <li>b</li>\n      </ul>\n    </li>\n"));
    assert!(!html.contains("[r]: /t"));
    assert!(html.ends_with("<p><a href=\"/t\">x</a></p>"));
}

#[test]
fn literal_angle_text_does_not_hide_real_root_item_columns() {
    let cases = [
("root-columns-0","- a\n- - b\n\n    [r]: /x\n\nx > [y][r]\n","<ul>\n  <li>a</li>\n  <li>\n    <ul>\n      <li>b</li>\n    </ul>\n  </li>\n</ul>\n<p>x &gt; <a href=\"/x\">y</a></p>"),
("root-columns-1","- a\n\n  - b\n    c\n- d\n> q\n","<ul>\n  <li>a\n    <ul>\n      <li>b\nc</li>\n    </ul>\n  </li>\n  <li>d</li>\n</ul>\n<blockquote><p>q</p></blockquote>"),
("root-columns-2",":: t\n\n- a\n- - b\n\n    [r]: /x\n\nx > [y][r]\n","<dl>\n  <dt>t</dt>\n</dl>\n<ul>\n  <li>a</li>\n  <li>\n    <ul>\n      <li>b</li>\n    </ul>\n  </li>\n</ul>\n<p>x &gt; <a href=\"/x\">y</a></p>"),
("root-columns-3","- a\n- - ```\n    [r]: /x\n    ```\n\nx > [y][r]\n","<ul>\n  <li>a</li>\n  <li>\n    <ul>\n      <li>\n        <pre><code>[r]: /x\n</code></pre>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>x &gt; [y][r]</p>"),
];
    for (name, source, expected) in cases {
        assert_eq!(
            render_html(&parse(source)).unwrap().trim(),
            expected,
            "{name}"
        );
    }
}

#[test]
fn flush_left_attachment_ends_the_inherited_quote_paragraph() {
    let cases = [
("> a\n+\n- inner\n  [r]: /url\n\nSee [r][]\n","<blockquote>\n  <p>a</p>\n  <ul>\n    <li>inner</li>\n  </ul>\n</blockquote>\n<p>See <a href=\"/url\">r</a></p>"),
("> a\n+\n- inner\n  [^n]: note\n\nSee [^n]\n","<blockquote>\n  <p>a</p>\n  <ul>\n    <li>inner</li>\n  </ul>\n</blockquote>\n<p>See <a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>note<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"),
("> a\n> +\n> - inner\n>   [r]: /url\n\nSee [r][]\n","<blockquote><p>a\n+\n- inner</p></blockquote>\n<p>See <a href=\"/url\">r</a></p>"),
("> a\n  +\n- inner\n  [r]: /url\n\nSee [r][]\n","<blockquote><p>a\n+\n- inner</p></blockquote>\n<p>See <a href=\"/url\">r</a></p>"),
];
    for (source, expected) in cases {
        assert_eq!(
            render_html(&parse(source)).unwrap().trim(),
            expected,
            "{source}"
        );
    }
}
