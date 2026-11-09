// carve#5401: the blank ends an unclosed attached fence before the next line
// selects an owner. The follower's column then closes or resumes the item.

#[test]
fn a_band_heading_after_an_ended_attached_fence_opens_outside_the_item() {
    let source = "* +\n```\n\n # q\n";
    let expected = "<ul>\n  <li>\n    <pre><code></code></pre>\n  </li>\n</ul>\n<section id=\"q\">\n  <h1>q</h1>\n</section>";
    assert_eq!(carve::to_html(source), expected);
}

#[test]
fn a_nested_band_quote_after_an_ended_attached_fence_opens_in_the_outer_item() {
    let source = "- a\n  * +\n  ```\n\n   > q\n";
    let expected = "<ul>\n  <li><p>a</p>\n    <ul>\n      <li>\n        <pre><code></code></pre>\n      </li>\n    </ul>\n    <blockquote><p>q</p></blockquote>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source), expected);
}

#[test]
fn a_closed_attached_fence_keeps_its_blank_as_payload() {
    let source = "* +\n```\n\n # q\n```\n";
    assert!(carve::to_html(source).contains("<pre><code>\n # q\n</code></pre>"));
}

#[test]
fn a_band_sibling_after_an_ended_attached_fence_joins_the_list() {
    let source = "* +\n```\n\n * q\n";
    let expected =
        "<ul>\n  <li>\n    <pre><code></code></pre>\n  </li>\n  <li><p>q</p></li>\n</ul>";
    assert_eq!(carve::to_html(source), expected);
}

#[test]
fn a_shifted_host_closer_leaves_the_heading_to_the_document() {
    let source = "1. +\n```\n\n ```\n  # h\n";
    let expected = "<ol>\n  <li>\n    <pre><code>\n</code></pre>\n  </li>\n</ol>\n<section id=\"h\">\n  <h1>h</h1>\n</section>";
    assert_eq!(carve::to_html(source), expected);
}

#[test]
fn shifted_attached_closer_variant_sweep() {
    let mut checked = 0;
    for (marker, closer_columns, heading_columns) in [
        ("- +", &[1usize][..], 1usize),
        ("1. +", &[1usize, 2][..], 2usize),
    ] {
        for fence in ['`', '~'] {
            for width in 3..=5 {
                let run = fence.to_string().repeat(width);
                for &closer_column in closer_columns {
                    for level in 1..=3 {
                        let hashes = "#".repeat(level);
                        for word in 0..20 {
                            let input = format!(
                                "{marker}\n{run}\n\n{}{run}\n{}{hashes} h{word}\n",
                                " ".repeat(closer_column),
                                " ".repeat(heading_columns)
                            );
                            let html = carve::to_html(&input);
                            assert_eq!(html.matches("<pre>").count(), 1, "{input:?}\n{html}");
                            assert!(
                                html.contains(&format!("<h{level}>h{word}</h{level}>")),
                                "{input:?}\n{html}"
                            );
                            checked += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(checked, 1080);
}

#[test]
fn a_plain_payload_line_does_not_hide_the_shifted_closer() {
    let input = "- - +\n  ```\n\n  x\n   ```\n";
    let html = carve::to_html(input);
    assert!(html.contains("<pre><code>\nx\n</code></pre>"), "{html}");
}

#[test]
fn a_fence_at_the_host_content_column_opens_after_the_blank() {
    let input = "- +\n```\na\n\n  ```\n  c\n";
    let html = carve::to_html(input);
    assert_eq!(html.matches("<pre>").count(), 2, "{html}");
    assert!(html.contains("<pre><code>a\n</code></pre>"), "{html}");
    assert!(html.contains("<pre><code>c\n</code></pre>"), "{html}");
}

#[test]
fn a_shifted_run_before_any_blank_remains_fence_payload() {
    let input = "- foo\n  - +\n  ```\n  x\n   ```\n";
    let html = carve::to_html(input);
    assert!(html.contains("<pre><code>x\n ```\n</code></pre>"), "{html}");
}
