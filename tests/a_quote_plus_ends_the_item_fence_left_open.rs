//! A quote's next `+` is a marker, never payload of the item fence the last
//! attached block left open: the fence ends before it (PART 9 §17 L3, PART 0
//! S1, markup-carve/carve#5046), and its nodes keep their spans
//! (markup-carve/carve-rs#3873).

use serde_json::Value;

fn ast(source: &str) -> Value {
    let options = carve::Options {
        positions: true,
        ..Default::default()
    };
    serde_json::from_str(&carve::to_json(&carve::parse_with_options(
        source, &options,
    )))
    .expect("the serializer emits JSON")
}

fn spans(node: &Value, out: &mut Vec<(String, Option<(u64, u64)>)>) {
    match node {
        Value::Array(items) => items.iter().for_each(|item| spans(item, out)),
        Value::Object(fields) => {
            if let Some(Value::String(ty)) = fields.get("type") {
                let pos = fields.get("pos").map(|pos| {
                    (
                        pos["startOffset"].as_u64().unwrap(),
                        pos["endOffset"].as_u64().unwrap(),
                    )
                });
                out.push((ty.clone(), pos));
            }
            for (key, value) in fields {
                if key != "pos" {
                    spans(value, out);
                }
            }
        }
        _ => {}
    }
}

fn span(source: &str, ty: &str, nth: usize) -> Option<(u64, u64)> {
    let mut found = Vec::new();
    spans(&ast(source), &mut found);
    found
        .into_iter()
        .filter(|(t, _)| t == ty)
        .nth(nth)
        .unwrap_or_else(|| panic!("no {ty} #{nth} in {source:?}"))
        .1
}

#[test]
fn the_fence_holds_no_blank_for_the_marker() {
    let whitespace = regex::Regex::new(r">\s+<").unwrap();
    for (source, html) in [
        (
            "> a\n+\n- ``` js\n  x\n+\n- y\n",
            "<blockquote><p>a</p><ul><li><pre><code class=\"language-js\">x\n</code></pre></li><li><p>y</p></li></ul></blockquote>",
        ),
        (
            "> a\n+\n1. ```\n   x\n+\ny\n",
            "<blockquote><p>a</p><ol><li><pre><code>x\n</code></pre></li></ol><p>y</p></blockquote>",
        ),
        (
            "> a\n+\n- ``` js\n+\n- y\n",
            "<blockquote><p>a</p><ul><li><pre><code class=\"language-js\"></code></pre></li><li><p>y</p></li></ul></blockquote>",
        ),
        (
            "> a\n+\n:: t\n: ```\n  x\n+\n:: u\n",
            "<blockquote><p>a</p><dl><dt>t</dt><dd><pre><code>x\n</code></pre></dd><dt>u</dt></dl></blockquote>",
        ),
    ] {
        assert_eq!(
            whitespace.replace_all(&carve::to_html(source), "><").trim(),
            html,
            "{source:?}"
        );
    }
}

#[test]
fn the_item_and_its_fence_keep_their_spans() {
    let source = "> a\n+\n- ``` js\n  x\n+\n- y\n";
    assert_eq!(span(source, "list_item", 0), Some((6, 18)));
    assert_eq!(span(source, "code_block", 0), Some((8, 18)));

    let nested = "> a\n+\n- b\n  - ~~~\n    x\n+\n- y\n";
    assert_eq!(span(nested, "list", 1), Some((12, 23)));
    assert_eq!(span(nested, "list_item", 1), Some((12, 23)));
    assert_eq!(span(nested, "code_block", 0), Some((14, 23)));
}
