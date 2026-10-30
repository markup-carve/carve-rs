use carve::{to_html, to_html_with_options, Options};

#[test]
fn flush_left_descriptions_below_a_quote_match_the_checker() {
    let cases = [
        (
            "> :: t\n> : a\n: b\n",
            "<blockquote>\n  <dl>\n    <dt>t</dt>\n    <dd>a\n: b</dd>\n  </dl>\n</blockquote>",
        ),
        (
            // The marker at quote column zero ends the list item. The lazy
            // paragraph's one-space model difference is tracked in carve#3493.
            "> - :: t\n> : a\n: b\n",
            "<blockquote>\n  <ul>\n    <li>\n      <dl>\n        <dt>t</dt>\n      </dl>\n    </li>\n  </ul>\n  <p>: a\n: b</p>\n</blockquote>",
        ),
        (
            "> - :: t\n  : a\n: b\n",
            "<blockquote>\n  <ul>\n    <li>\n      <dl>\n        <dt>t</dt>\n      </dl>\n    </li>\n  </ul>\n</blockquote>\n<p>: a\n: b</p>",
        ),
        (
            "> :: t\n> : a\n> : b\n",
            "<blockquote>\n  <dl>\n    <dt>t</dt>\n    <dd>a</dd>\n    <dd>b</dd>\n  </dl>\n</blockquote>",
        ),
    ];
    for (source, expected) in cases {
        assert_eq!(to_html(source), expected, "{source:?}");
        assert_eq!(
            to_html_with_options(source, &Options::default().with_positions(true)),
            expected,
            "positions: {source:?}"
        );
    }
    let positioned =
        carve::parse_with_options(cases[0].0, &Options::default().with_positions(true));
    let tree: serde_json::Value = serde_json::from_str(&carve::to_json(&positioned)).unwrap();
    fn text_at<'a>(node: &'a serde_json::Value, value: &str) -> Option<&'a serde_json::Value> {
        if node["type"] == "text" && node["value"] == value {
            return Some(node);
        }
        match node {
            serde_json::Value::Object(fields) => {
                fields.values().find_map(|child| text_at(child, value))
            }
            serde_json::Value::Array(items) => items.iter().find_map(|child| text_at(child, value)),
            _ => None,
        }
    }
    let lazy_text = text_at(&tree, ": b").expect("the lazy description line is text");
    assert_eq!(lazy_text["pos"]["startLine"], 3);
    assert_eq!(lazy_text["pos"]["startColumn"], 1);
    assert_eq!(lazy_text["pos"]["endColumn"], 4);
}
