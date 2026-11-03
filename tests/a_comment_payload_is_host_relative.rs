//! A `%%%` payload keeps its columns past the host's content column; the
//! opener's indentation sets no base (PART 9 §28, CARVE-P9-060, ruled on
//! markup-carve/carve#4988).

use carve::{parse, to_carve, to_json};
use serde_json::Value;

fn block_comments(source: &str) -> Vec<String> {
    fn walk(node: &Value, out: &mut Vec<String>) {
        match node {
            Value::Object(map) => {
                if map.get("type") == Some(&Value::String("comment".into()))
                    && map.get("block") == Some(&Value::Bool(true))
                {
                    out.push(map["content"].as_str().unwrap().to_owned());
                }
                map.values().for_each(|v| walk(v, out));
            }
            Value::Array(items) => items.iter().for_each(|v| walk(v, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(
        &serde_json::from_str(&to_json(&parse(source))).unwrap(),
        &mut out,
    );
    out
}

#[test]
fn an_over_indented_opener_sets_no_payload_base() {
    let cases: &[(&str, &[&str])] = &[
        // Corpus 508-4.
        (
            "- head\n\n    %%%\n    a\n%%%\n    %%%\n    b\n    %%%\n\n  tail\n",
            &["  a", "  b"],
        ),
        ("- x\n\n    %%%\n    a\n     b\n    %%%\n", &["  a\n   b"]),
        ("1. x\n\n     %%%\n     a\n   %%%\n", &["  a"]),
        (">   %%%\n>   a\n>   %%%\n", &["  a"]),
        (":: c\n  %%%\n  hidden\n  %%%\n  more\n", &["  hidden"]),
        // At the host's column the payload is unchanged.
        ("- x\n\n  %%%\n  a\n  %%%\n", &["a"]),
    ];
    for (source, expected) in cases {
        assert_eq!(block_comments(source), *expected, "on {source:?}");
    }
}

#[test]
fn the_writer_keeps_the_host_relative_payload() {
    let source = "- head\n\n    %%%\n    a\n%%%\n    %%%\n    b\n    %%%\n\n  tail\n";
    let written = to_carve(source);
    assert_eq!(
        written,
        "- head\n\n  %%%\n    a\n  %%%\n\n  %%%\n    b\n  %%%\n\n  tail\n"
    );
    assert_eq!(block_comments(&written), block_comments(source));
    assert_eq!(to_carve(&written), written);
}
