//! A refused `+` renders nothing and is no node, so the container that held its
//! placeholder ends at its last placed child, or on the one marker right below
//! it (CARVE-P12-014, markup-carve/carve-rs#3870).

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

fn spans(node: &Value, ty: &str, out: &mut Vec<(u64, u64)>) {
    match node {
        Value::Array(items) => items.iter().for_each(|item| spans(item, ty, out)),
        Value::Object(fields) => {
            if fields.get("type").and_then(Value::as_str) == Some(ty) {
                if let Some(pos) = fields.get("pos") {
                    out.push((
                        pos["startOffset"].as_u64().unwrap(),
                        pos["endOffset"].as_u64().unwrap(),
                    ));
                }
            }
            for (key, value) in fields {
                if key != "pos" {
                    spans(value, ty, out);
                }
            }
        }
        _ => {}
    }
}

fn first(source: &str, ty: &str) -> (u64, u64) {
    let mut found = Vec::new();
    spans(&ast(source), ty, &mut found);
    found[0]
}

#[test]
fn an_item_ends_on_the_first_refused_plus_below_its_content() {
    for (source, ty, span) in [
        ("- q\n+\n+\n\n- r\n", "list_item", (0, 5)),
        ("- q\n+\n+\n- r\n", "list_item", (0, 5)),
        ("- q\n+\n+\n+\n\n- r\n", "list_item", (0, 5)),
        ("- q\n+\n\n+\n\n- r\n", "list_item", (0, 5)),
        ("- q\n+\n\n- r\n", "list_item", (0, 5)),
        ("> - q\n> +\n> +\n>\n> - r\n", "list_item", (2, 9)),
        (":: t\n: q\n+\n+\n\n:: u\n: v\n", "definition_description", (5, 10)),
    ] {
        assert_eq!(first(source, ty), span, "{source:?}");
    }
}

#[test]
fn the_list_follows_its_last_item() {
    assert_eq!(first("- q\n+\n+\n", "list"), (0, 5));
    let mut lists = Vec::new();
    spans(&ast("- a\n  - q\n  +\n  +\n"), "list", &mut lists);
    assert_eq!(lists, [(0, 17), (6, 13)]);
}
