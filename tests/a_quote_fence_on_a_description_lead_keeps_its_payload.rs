//! A quote opened on a description's lead line (`: > ````) holds the fence's
//! payload: a definition-shaped payload line is code, not a definition.

#[test]
fn a_lead_quote_fence_keeps_definition_shaped_payload() {
    for (source, expected) in [
        (
            ":: t\n: > ```\n  > [r]: /u\n\n[x][r]\n",
            "<dl>\n  <dt>t</dt>\n  <dd>\n    <blockquote>\n      <pre><code>[r]: /u\n</code></pre>\n    </blockquote>\n  </dd>\n</dl>\n<p>[x][r]</p>",
        ),
        (
            ":: t\n\n: > ```\n   > [^a]: note\n\n[^outside]: body\n",
            "<dl>\n  <dt>t</dt>\n  <dd>\n    <blockquote>\n      <pre><code>[^a]: note\n</code></pre>\n    </blockquote>\n  </dd>\n</dl>",
        ),
        (
            ":: t\n: > > ~~~\n  > > [r]: /u\n",
            "<dl>\n  <dt>t</dt>\n  <dd>\n    <blockquote>\n      <blockquote>\n        <pre><code>[r]: /u\n</code></pre>\n      </blockquote>\n    </blockquote>\n  </dd>\n</dl>",
        ),
        (
            ":: t\n: > ```\n  > [^n]: x\n  > ```\n\nsee [^n]\n",
            "<dl>\n  <dt>t</dt>\n  <dd>\n    <blockquote>\n      <pre><code>[^n]: x\n</code></pre>\n    </blockquote>\n  </dd>\n</dl>\n<p>see [^n]</p>",
        ),
        (
            "> :: t\n> : > ````\n>   > *[A]: x\n",
            "<blockquote>\n  <dl>\n    <dt>t</dt>\n    <dd>\n      <blockquote>\n        <pre><code>*[A]: x\n</code></pre>\n      </blockquote>\n    </dd>\n  </dl>\n</blockquote>",
        ),
    ] {
        assert_eq!(carve::to_html(source).trim(), expected, "{source:?}");
    }
}
