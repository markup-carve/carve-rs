#[test]
fn paragraph_after_an_attached_unclosed_fence_takes_a_lazy_line() {
    let source = "- a\n  - +\n  ```\n\n    p\ntail\n";
    let expected = "<ul>\n  <li>a\n    <ul>\n      <li>\n        <pre><code></code></pre>\n        <p>p\ntail</p>\n      </li>\n    </ul>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source), expected);
}

#[test]
fn line_left_of_attached_host_does_not_pull_a_later_closer() {
    let source = "- x\n+\n```\na\n\nb\n``\n>> \n  ```\nz\n";
    let expected = "<ul>\n  <li>x\n    <pre><code>a\n</code></pre>\n  </li>\n</ul>\n<p>b\n<code>\n&gt;&gt;\n```\nz</code></p>";
    assert_eq!(carve::to_html(source), expected);
}
use carve::to_html;

#[test]
fn an_unpaired_fence_folds_in_every_surviving_owner() {
    let rows = [
        (
            "- - +\n  ```\n\nx\n   ~~~\n",
            "<ul>\n  <li>\n    <ul>\n      <li>\n        <pre><code></code></pre>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>x\n~~~</p>",
        ),
        (
            "- - - +\n    ```\n\n  tail\n     ~~~\n",
            "<ul>\n  <li>\n    <ul>\n      <li>\n        <ul>\n          <li>\n            <pre><code></code></pre>\n          </li>\n        </ul>\n      </li>\n    </ul>\n    <p>tail\n~~~</p>\n  </li>\n</ul>",
        ),
    ];
    for (input, expected) in rows {
        assert_eq!(to_html(input).trim_end(), expected, "{input}");
    }
}

#[test]
fn a_paired_fence_and_a_new_paragraph_still_open_blocks() {
    let paired = "- - +\n  ```\n\nx\n~~~\ncode\n~~~\n";
    assert!(carve::to_html(paired).contains("<p>x</p>\n<pre><code>code\n</code></pre>"));

    let separated = "- - +\n  ```\n\nx\n\n~~~\n";
    assert!(carve::to_html(separated).contains("<p>x</p>\n<pre><code></code></pre>"));
}

#[test]
fn unpaired_fence_variant_sweep() {
    let mut checked = 0;
    for (host, opener_indent, paragraph_indent, follower_indent) in [
        ("- - +", "  ", "", "   "),
        ("- - - +", "    ", "  ", "     "),
    ] {
        for opener in ['`', '~'] {
            let follower = if opener == '`' { '~' } else { '`' };
            for opener_width in 3..=5 {
                for follower_width in 3..=5 {
                    for word in 0..30 {
                        let opener_run = opener.to_string().repeat(opener_width);
                        let follower_run = follower.to_string().repeat(follower_width);
                        let input = format!(
                            "{host}\n{opener_indent}{opener_run}\n\n{paragraph_indent}tail{word}\n{follower_indent}{follower_run}\n"
                        );
                        let html = carve::to_html(&input);
                        let expected = format!("<p>tail{word}\n");
                        assert!(html.contains(&expected), "{input:?}\n{html}");
                        assert_eq!(html.matches("<pre>").count(), 1, "{input:?}\n{html}");
                        checked += 1;
                    }
                }
            }
        }
    }
    assert_eq!(checked, 1080);
}
