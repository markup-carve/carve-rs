use carve::{parse_with_options, render_html, to_html, Options};

fn assert_html(source: &str, expected: &str) {
    assert_eq!(to_html(source), expected, "source: {source:?}");
    let source_attribute = regex::Regex::new(r#" data-source-line="[0-9]+""#).unwrap();
    for (positions, source_lines) in [(true, false), (false, true), (true, true)] {
        let options = Options {
            positions,
            source_lines,
            ..Options::default()
        };
        let html = render_html(&parse_with_options(source, &options)).unwrap();
        assert_eq!(
            source_attribute.replace_all(&html, ""),
            expected,
            "positions={positions}, source_lines={source_lines}, source: {source:?}"
        );
    }
}

#[test]
fn a_comment_closer_uses_the_openers_column_for_following_text() {
    for (marker, column, tag, attrs) in [
        ("- ", 2, "ul", ""),
        ("1. ", 3, "ol", ""),
        ("10. ", 4, "ol", " start=\"10\""),
    ] {
        for width in [3, 5] {
            let run = "%".repeat(width);
            for closer in 0..=8 {
                for opener in [1, column] {
                    let source = format!(
                        "{marker}item\n{}{run}\n{}hidden\n{}{run}\ntail\n",
                        " ".repeat(opener),
                        " ".repeat(opener),
                        " ".repeat(closer)
                    );
                    let expected = if opener < column {
                        format!("<{tag}{attrs}>\n  <li>item\n    tail\n  </li>\n</{tag}>")
                    } else {
                        format!("<{tag}{attrs}>\n  <li>item</li>\n</{tag}>\n<p>tail</p>")
                    };
                    assert_html(&source, &expected);
                }
            }
        }
    }
}

#[test]
fn a_marker_line_comment_keeps_its_closer_and_leaves_no_paragraph() {
    for closer in 0..=8 {
        for follower in ["tail", " tail"] {
            assert_html(
                &format!("- %%%\n  hidden\n{}%%%\n{follower}\n", " ".repeat(closer)),
                "<ul>\n  <li></li>\n</ul>\n<p>tail</p>",
            );
        }
    }
}

#[test]
fn a_code_fence_after_a_below_column_comment_keeps_its_lazy_payload() {
    for closer in 0..=8 {
        assert_html(
            &format!(
                "- item\n %%%\n hidden\n{}%%%\n  ```\ntail\n",
                " ".repeat(closer)
            ),
            "<ul>\n  <li>item\n    <pre><code>tail\n</code></pre>\n  </li>\n</ul>",
        );
    }
}
