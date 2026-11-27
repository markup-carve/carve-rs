//! carve#6896: a block snippet use takes no attribute line (C2-B-096). The line
//! above it is a paragraph of its own (C2-B-092) in every container.
use carve::api::json::call;
use carve::options::Options;
use serde_json::{Value, json};

fn formatted(source: &str) -> String {
    let r: Value = serde_json::from_str(&call(
        &json!({"call": "format", "source": source}).to_string(),
    ))
    .unwrap();
    r["ok"]["source"].as_str().unwrap().to_string()
}

fn children(source: &str) -> Value {
    let tree: Value = serde_json::from_str(&carve::tree_json(source, &Options::default())).unwrap();
    tree["children"].clone()
}

#[test]
fn the_line_above_a_block_use_is_a_paragraph() {
    let opts = Options::default();
    let cases = [
        (
            "{$v}: X\n\n{.k :en}\n{{ $v }}\n",
            "<p>{.k :en}</p>\n<p>X</p>",
        ),
        ("{$v}: X\n\n{#i}\n{{ $v }}\n", "<p>{#i}</p>\n<p>X</p>"),
        (
            "{$v}: X\n\n{.a}\n{#i}\n{.b :en}\n{{ $v }}\n",
            "<p>{.a}\n{#i}\n{.b :en}</p>\n<p>X</p>",
        ),
        (
            "{$v}: X\n\n> {.k}\n> {{ $v }}\n",
            "<blockquote>\n  <p>{.k}</p>\n  <p>X</p>\n</blockquote>",
        ),
        (
            "{$v}: X\n\n::: d\n{.k}\n{{ $v }}\n:::\n",
            "<div class=\"d\">\n  <p>{.k}</p>\n  <p>X</p>\n</div>",
        ),
    ];
    for (source, html) in cases {
        assert_eq!(carve::render(source, &opts), html, "{source}");
    }
}

#[test]
fn the_use_carries_no_attrs_and_the_writer_keeps_the_line() {
    let source = "{$v}: X\n\n{.k :en}\n{{ $v }}\n";
    let kids = children(source);
    assert_eq!(kids[1]["type"], "paragraph");
    assert_eq!(kids[2]["type"], "snippet_use");
    assert!(kids[2].get("attrs").is_none() && kids[2].get("lang").is_none());
    let out = formatted(source);
    assert_eq!(out, "{$v}: X\n\n{.k :en}\n\n{{ $v }}\n");
    let types = |v: &Value| -> Vec<Value> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|n| n["type"].clone())
            .collect()
    };
    assert_eq!(types(&children(&out)), types(&kids), "{out}");
}

#[test]
fn other_blocks_on_the_line_still_take_it() {
    let opts = Options::default();
    // C2-B-091: the list and the paragraph take the line; a definition does too (carve#6865).
    assert_eq!(
        carve::render("{$v}: X\n\n{.k}\n- {{ $v }}\n", &opts),
        "<ul class=\"k\">\n  <li>X</li>\n</ul>"
    );
    assert_eq!(
        carve::render("{$v}: X\n\n{.k}\n{{ $v }} t\n", &opts),
        "<p class=\"k\">X t</p>"
    );
    assert_eq!(
        carve::render("{.k}\n{$v}: X\n\n{.m}\n{{ $v }}\n", &opts),
        "<p>{.m}</p>\n<p class=\"k\">X</p>"
    );
}
