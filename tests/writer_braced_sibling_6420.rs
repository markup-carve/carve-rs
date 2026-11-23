//! C2-W-069 (carve#6420): a same-kind emphasis or strong after a braced sibling is written bare
//! where the bare form pairs there, and still braced where its bare mark glues to the one before.

use carve::api::{self, Carve, Options, WriteOptions};
use serde_json::{Value, json};

fn write(children: &[Value]) -> String {
    let tree = json!({ "type": "document", "astVersion": "2.0",
        "children": [{ "type": "paragraph", "children": children }] });
    let tree = api::decode_tree(&tree.to_string()).expect("a valid tree");
    let out = Carve::new()
        .write(&tree, &WriteOptions::default())
        .expect("a write");
    let back = Carve::new()
        .parse(out.as_str(), &Options::default())
        .expect("a parse");
    let mut got: Value = serde_json::from_str(&api::encode_tree(&back)).expect("json");
    strip(&mut got);
    assert_eq!(
        got["children"][0]["children"],
        json!(children),
        "{out:?} reads back"
    );
    out
}

fn strip(v: &mut Value) {
    match v {
        Value::Object(map) => {
            map.remove("pos");
            map.values_mut().for_each(strip);
        }
        Value::Array(items) => items.iter_mut().for_each(strip),
        _ => {}
    }
}

fn pair(kind: &str, v: &str) -> Value {
    json!({ "type": kind, "children": [{ "type": "text", "value": v }] })
}

#[test]
fn the_ticket_reproductions() {
    assert_eq!(
        write(&[pair("emphasis", " a"), pair("emphasis", "a")]),
        "{/ a/}/a/\n"
    );
    assert_eq!(
        write(&[pair("strong", " b"), pair("strong", "a")]),
        "{* b*}*a*\n"
    );
    let word = json!({ "type": "text", "value": "p" });
    let escaped = [word, pair("emphasis", "xx"), pair("emphasis", "{")];
    assert_eq!(write(&escaped), "p{/xx/}/\\{/\n");
}

#[test]
fn a_bare_sibling_still_braces_the_next_one() {
    let three = [
        pair("emphasis", " a"),
        pair("emphasis", "b"),
        pair("emphasis", "c"),
    ];
    assert_eq!(write(&three), "{/ a/}/b/{/c/}\n");
    let four = [
        pair("strong", "a "),
        pair("strong", "b"),
        pair("strong", "c"),
        pair("strong", "d"),
    ];
    assert_eq!(write(&four), "{*a *}*b*{*c*}*d*\n");
}

#[test]
fn other_kinds_between_keep_their_own_forms() {
    let mixed = [
        pair("emphasis", " a"),
        pair("strong", "b"),
        pair("emphasis", "c"),
    ];
    assert_eq!(write(&mixed), "{/ a/}*b*/c/\n");
}
