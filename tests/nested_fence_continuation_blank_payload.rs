use carve::{to_html, to_html_with_options, Options};

#[test]
fn nested_fence_continuation_keeps_every_blank_and_attaches_tail() {
    let cases = [
        (
            "- - ```\n\n\n  +\ntail\n",
            "<ul>\n  <li>\n    <ul>\n      <li>\n        <pre><code>\n\n</code></pre>\n        tail\n      </li>\n    </ul>\n  </li>\n</ul>",
        ),
        (
            "- - ```\n\n  +\n> q\n",
            "<ul>\n  <li>\n    <ul>\n      <li>\n        <pre><code>\n</code></pre>\n      </li>\n    </ul>\n  </li>\n</ul>\n<blockquote><p>q</p></blockquote>",
        ),
        (
            "- - ```\n\n  +\n- s\n",
            "<ul>\n  <li>\n    <ul>\n      <li>\n        <pre><code>\n</code></pre>\n      </li>\n    </ul>\n  </li>\n  <li><p>s</p></li>\n</ul>",
        ),
        (
            "- - ```\n\n  +\n~~~\ny\n",
            "<ul>\n  <li>\n    <ul>\n      <li>\n        <pre><code>\n</code></pre>\n        ~~~\ny\n      </li>\n    </ul>\n  </li>\n</ul>",
        ),
        (
            "- 1. ```\n\n\n  +\ntail\n",
            "<ul>\n  <li>\n    <ol>\n      <li>\n        <pre><code>\n\n</code></pre>\n        tail\n      </li>\n    </ol>\n  </li>\n</ul>",
        ),
        (
            "- - - ```\n\n\n    +\ntail\n",
            "<ul>\n  <li>\n    <ul>\n      <li>\n        <ul>\n          <li>\n            <pre><code>\n\n</code></pre>\n            tail\n          </li>\n        </ul>\n      </li>\n    </ul>\n  </li>\n</ul>",
        ),
        (
            "- - [ ] ```\n\n\n  +\ntail\n",
            "<ul>\n  <li>\n    <ul class=\"task-list\">\n      <li><input type=\"checkbox\" disabled> \n        <pre><code>\n\n</code></pre>\n        tail\n      </li>\n    </ul>\n  </li>\n</ul>",
        ),
    ];
    for (source, expected) in cases {
        assert_eq!(to_html(source), expected, "{source:?}");
        assert_eq!(
            to_html_with_options(source, &Options::default().with_positions(true)),
            expected,
            "{source:?} with positions"
        );
    }
}
