use carve::{parse, parse_with_options, render_html, to_carve_with_options, Options};

#[test]
fn nested_description_fences_keep_definition_shaped_payload() {
    let cases = [
(":: t\n: - ```\n[r]: /u\n[^a]: note\n```\n","<dl>\n  <dt>t</dt>\n  <dd>\n    <ul>\n      <li>\n        <pre><code>[r]: /u\n[^a]: note\n```\n</code></pre>\n      </li>\n    </ul>\n  </dd>\n</dl>"),
(":: t\n: - ```x\n[r]: /u\n[^a]: note\n```\n","<dl>\n  <dt>t</dt>\n  <dd>\n    <ul>\n      <li>\n        <pre><code class=\"language-x\">[r]: /u\n[^a]: note\n```\n</code></pre>\n      </li>\n    </ul>\n  </dd>\n</dl>"),
(":: t\n: - ~~~\n[r]: /u\n[^a]: note\n~~~\n","<dl>\n  <dt>t</dt>\n  <dd>\n    <ul>\n      <li>\n        <pre><code>[r]: /u\n[^a]: note\n~~~\n</code></pre>\n      </li>\n    </ul>\n  </dd>\n</dl>"),
(":: t\n: ```\n[r]: /u\n[^a]: note\n```\n","<dl>\n  <dt>t</dt>\n  <dd>\n    <pre><code></code></pre>\n  </dd>\n</dl>\n<pre><code></code></pre>"),
(":: t\n: - - ```\n[r]: /u\n[^a]: note\n```\n","<dl>\n  <dt>t</dt>\n  <dd>\n    <ul>\n      <li>\n        <ul>\n          <li>\n            <pre><code>[r]: /u\n[^a]: note\n```\n</code></pre>\n          </li>\n        </ul>\n      </li>\n    </ul>\n  </dd>\n</dl>"),
(":: t\n: - ```\n    literal\n    ```\n\n[r]: /u\n[^a]: note\n\n[x][r] [^a]\n","<dl>\n  <dt>t</dt>\n  <dd>\n    <ul>\n      <li>\n        <pre><code>literal\n</code></pre>\n      </li>\n    </ul>\n  </dd>\n</dl>\n<p><a href=\"/u\">x</a> <a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>note<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"),
(": - ```\n[r]: /u\n[^a]: note\n```\n","<p>: - <code></code></p>\n<pre><code></code></pre>"),
("p\n: - ```\n[r]: /u\n[^a]: note\n```\n","<p>p\n: - <code></code></p>\n<pre><code></code></pre>"),
("> :: t\n> : - ```\n> [r]: /u\n> [^a]: note\n> ```\n","<blockquote>\n  <dl>\n    <dt>t</dt>\n    <dd>\n      <ul>\n        <li>\n          <pre><code>[r]: /u\n[^a]: note\n```\n</code></pre>\n        </li>\n      </ul>\n    </dd>\n  </dl>\n</blockquote>"),
];
    for (source, expected) in cases {
        for positions in [false, true] {
            let options = Options::default().with_positions(positions);
            let doc = parse_with_options(source, &options);
            assert_eq!(render_html(&doc).unwrap().trim(), expected, "{source}");
            let formatted = to_carve_with_options(source, &options);
            assert_eq!(
                render_html(&parse(&formatted)).unwrap().trim(),
                expected,
                "{source}: {formatted}"
            );
        }
    }
}

#[test]
fn an_owned_description_closer_preserves_literal_code_content() {
    let cases = [
        (
            ":: t\n: - ```\n    [r]: /u\n    [^a]: note\n    ```\n",
            "[r]: /u\n[^a]: note\n",
        ),
        (
            ":: t\n: - ~~~\n    [r]: /u\n    [^a]: note\n    ~~~\n",
            "[r]: /u\n[^a]: note\n",
        ),
        (
            ":: t\n: - ```\n\n    [r]: /u\n    [^a]: note\n    ```\n\n[x][r]\n",
            "\n[r]: /u\n[^a]: note\n",
        ),
    ];
    for (source, payload) in cases {
        for positions in [false, true] {
            let options = Options::default().with_positions(positions);
            let doc = parse_with_options(source, &options);
            let html = render_html(&doc).unwrap();
            assert!(
                html.contains(&format!("<code>{payload}</code>")),
                "{source}: {html}"
            );
            assert!(doc.footnote_defs.is_empty());
            assert!(!html
                .chars()
                .any(|ch| ('\u{e000}'..='\u{f8ff}').contains(&ch)));
            let formatted = to_carve_with_options(source, &options);
            let reparsed = render_html(&parse(&formatted)).unwrap();
            assert!(
                reparsed.contains(&format!("<code>{payload}</code>")),
                "{source}: {formatted}"
            );
        }
    }
}

#[test]
fn a_blank_boundary_distinguishes_payload_from_following_definitions() {
    let cases = [
(":: t\n: - ```\n[r]: /u\n```\n[z]: /v\n\n[x][z]\n","<dl>\n  <dt>t</dt>\n  <dd>\n    <ul>\n      <li>\n        <pre><code>[r]: /u\n```\n[z]: /v\n</code></pre>\n      </li>\n    </ul>\n  </dd>\n</dl>\n<p>[x][z]</p>"),
(":: t\n: - ```\n[r]: /u\n```\n\n[z]: /v\n\n[x][z]\n","<dl>\n  <dt>t</dt>\n  <dd>\n    <ul>\n      <li>\n        <pre><code>[r]: /u\n```\n</code></pre>\n      </li>\n    </ul>\n  </dd>\n</dl>\n<p><a href=\"/v\">x</a></p>"),
("> :: t\n> : - ```\n> [r]: /u\n> ```\n> [z]: /v\n>\n> [x][z]\n","<blockquote>\n  <dl>\n    <dt>t</dt>\n    <dd>\n      <ul>\n        <li>\n          <pre><code>[r]: /u\n```\n[z]: /v\n</code></pre>\n        </li>\n      </ul>\n    </dd>\n  </dl>\n  <p>[x][z]</p>\n</blockquote>"),
("> :: t\n> : - ```\n> [r]: /u\n> ```\n>\n> [z]: /v\n>\n> [x][z]\n","<blockquote>\n  <dl>\n    <dt>t</dt>\n    <dd>\n      <ul>\n        <li>\n          <pre><code>[r]: /u\n```\n</code></pre>\n        </li>\n      </ul>\n    </dd>\n  </dl>\n  <p><a href=\"/v\">x</a></p>\n</blockquote>"),
("> > :: t\n> > : - ```\n> > [r]: /u\n> > ```\n> > [z]: /v\n> >\n> > [x][z]\n","<blockquote>\n  <blockquote>\n    <dl>\n      <dt>t</dt>\n      <dd>\n        <ul>\n          <li>\n            <pre><code>[r]: /u\n```\n[z]: /v\n</code></pre>\n          </li>\n        </ul>\n      </dd>\n    </dl>\n    <p>[x][z]</p>\n  </blockquote>\n</blockquote>"),
("> > :: t\n> > : - ```\n> > [r]: /u\n> > ```\n> >\n> > [z]: /v\n> >\n> > [x][z]\n","<blockquote>\n  <blockquote>\n    <dl>\n      <dt>t</dt>\n      <dd>\n        <ul>\n          <li>\n            <pre><code>[r]: /u\n```\n</code></pre>\n          </li>\n        </ul>\n      </dd>\n    </dl>\n    <p><a href=\"/v\">x</a></p>\n  </blockquote>\n</blockquote>"),
 ];
    for (source, expected) in cases {
        for positions in [false, true] {
            let options = Options::default().with_positions(positions);
            assert_eq!(
                render_html(&parse_with_options(source, &options))
                    .unwrap()
                    .trim(),
                expected,
                "{source}"
            );
            let formatted = to_carve_with_options(source, &options);
            assert_eq!(
                render_html(&parse(&formatted)).unwrap().trim(),
                expected,
                "{source}: {formatted}"
            );
        }
    }
}
