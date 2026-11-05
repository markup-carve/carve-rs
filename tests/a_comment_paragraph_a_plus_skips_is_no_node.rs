//! A paragraph of only braced comments that a first-block `+` skips is read
//! as an erased line: it is no node of the host, so the host holds only the
//! block the marker attached (CARVE-P12-013, markup-carve/carve-rs#3872).

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

fn types(blocks: &Value) -> Vec<String> {
    blocks
        .as_array()
        .unwrap()
        .iter()
        .map(|block| block["type"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn the_host_holds_only_the_attached_block() {
    for (source, host) in [
        ("- +\n{% c %}\n# H\n", "/children/0/items/0/children"),
        ("1. +\n{%c%}\n# H\n", "/children/0/items/0/children"),
        ("- +\n{% c %}\n{% d %}\n# H\n", "/children/0/items/0/children"),
        ("- +\n{% c %} {% d %}\n# H\n", "/children/0/items/0/children"),
        ("> +\n{% c %}\n# H\n", "/children/0/children"),
        (":: t\n: +\n{% c %}\n# H\n", "/children/0/items/1/children"),
        ("> - +\n> {% c %}\n> # H\n", "/children/0/children/0/items/0/children"),
    ] {
        let doc = ast(source);
        assert_eq!(types(doc.pointer(host).unwrap()), ["heading"], "{source:?}");
    }
}

#[test]
fn an_unpositioned_parse_drops_it_too() {
    let doc: Value =
        serde_json::from_str(&carve::to_json(&carve::parse("- +\n{% c %}\n# H\n"))).unwrap();
    assert_eq!(
        types(doc.pointer("/children/0/items/0/children").unwrap()),
        ["heading"]
    );
}

#[test]
fn the_writer_drops_it_too() {
    assert_eq!(carve::to_carve("- +\n{% c %}\n# H\n"), "- # H\n");
}
