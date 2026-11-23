//! carve#6450: a block opener behind the marker of another block on the same line takes its own escape (C2-W-088),
//! for every container marker. carve#6451 (ruled option 1): a text whose only loss is a reported one did not read
//! back (C2-W-124), so the group takes the reported-openers form of C2-W-088. The expected bytes are carve-php's
//! (carve-php#5699) and carve-js's (carve-js#5442).

use carve::api::{self, Carve, Options, WriteOptions};
use serde_json::{Value, json};

fn text(v: &str) -> Value {
    json!({ "type": "text", "value": v })
}

fn soft() -> Value {
    json!({ "type": "soft_break" })
}

fn code(v: &str) -> Value {
    json!({ "type": "code", "value": v })
}

fn paragraph(children: &[Value]) -> Value {
    json!({ "type": "document", "astVersion": "2.0", "children": [{ "type": "paragraph", "children": children }] })
}

fn quote(children: &[Value]) -> Value {
    json!({ "type": "document", "astVersion": "2.0", "children": [
        { "type": "block_quote", "children": [{ "type": "paragraph", "children": children }] }
    ] })
}

fn bullet(children: &[Value]) -> Value {
    json!({ "type": "document", "astVersion": "2.0", "children": [{
        "type": "list", "marker": { "kind": "bullet", "char": "-" }, "spacing": "tight",
        "children": [{ "type": "list_item", "children": [{ "type": "paragraph", "children": children }] }]
    }] })
}

fn write(tree: &Value) -> (String, Vec<String>) {
    let tree = api::decode_tree(&tree.to_string()).expect("a valid tree");
    let written = Carve::new()
        .write_with_diagnostics(&tree, &WriteOptions::default())
        .expect("a write");
    let codes = written.diagnostics.iter().map(|e| e.code.to_string()).collect();
    (written.output, codes)
}

fn reads_back(tree: &Value, source: &str) -> bool {
    let carve = Carve::new();
    let parsed = carve.parse(source, &Options::default()).expect("a parse");
    let expected = api::decode_tree(&tree.to_string()).expect("a valid tree");
    let plain = api::EncodeOptions { positions: false };
    let children = |t: &api::Tree| {
        let v: Value = serde_json::from_str(&api::encode_tree_with(t, plain)).expect("tree JSON");
        v["children"].clone()
    };
    children(&parsed) == children(&expected)
}

#[test]
fn openers_behind_a_container_marker_take_their_own_escape() {
    for (first, expected) in [
        ("- ```x", "\\- \\```x\nc\n"),
        ("1. ```x", "1\\. \\```x\nc\n"),
        ("* ```x", "\\* \\```x\nc\n"),
        ("> - x", "\\> \\- x\nc\n"),
        ("- - x", "\\- \\- x\nc\n"),
        ("a. ```x", "a\\. \\```x\nc\n"),
        // A nested ordered marker takes the backslash at its delimiter (C2-W-090), a hyphen run at its second
        // hyphen (C2-W-099).
        ("> 1. x", "\\> 1\\. x\nc\n"),
        ("1) 2) x", "1\\) 2\\) x\nc\n"),
        ("- 1. ```x", "\\- 1\\. \\```x\nc\n"),
        ("- -- x", "\\- -\\- x\nc\n"),
        ("> - > x", "\\> \\- \\> x\nc\n"),
    ] {
        let tree = paragraph(&[text(first), soft(), text("c")]);
        assert_eq!(write(&tree), (expected.to_string(), vec![]), "{first:?}");
        assert!(reads_back(&tree, expected), "{first:?}");
    }
}

#[test]
fn a_fence_behind_a_marker_reads_the_rest_of_the_line() {
    // The strong span puts a backtick in the info string, so no fence opens behind the marker.
    let strong = json!({ "type": "strong", "children": [text("b x`")] });
    let tree = paragraph(&[text("- ``` "), strong]);
    assert_eq!(write(&tree), ("\\- ``` *b x`*\n".to_string(), vec![]));
}

#[test]
fn the_nested_escape_gives_no_kind() {
    let tree = paragraph(&[text("* ```"), soft(), text("q x`")]);
    assert_eq!(write(&tree), ("\\* \\```\nq x`\n".to_string(), vec![]));
}

#[test]
fn a_reported_loss_takes_the_reported_openers_form() {
    let unspellable = vec!["field-unspellable".to_string()];
    for (tree, expected) in [
        // Tree A of carve#6451.
        (paragraph(&[text("a"), soft(), text("  ` `"), soft(), text("c")]), "a\n  \\` `\nc\n"),
        // Tree B: the code span behind the lost blanks of an item line is reported, not written.
        (bullet(&[text("  x` `"), soft(), text("c")]), "-   x\\` `\n  c\n"),
        (quote(&[text("a"), soft(), text("  `x `"), soft(), text("c")]), "> a\n>   \\`x `\n> c\n"),
        (paragraph(&[text("/"), soft(), text("  ` ` d")]), "/\n  \\` ` d\n"),
        (paragraph(&[text("a"), soft(), text("  ` ` `"), soft(), text("c")]), "a\n  \\` \\` `\nc\n"),
        (quote(&[text("  x` `"), soft(), text("c")]), ">   x\\` `\n> c\n"),
        (paragraph(&[text("a"), soft(), text("  x` "), code("z")]), "a\n  x\\` `z`\n"),
        (paragraph(&[text("a b"), soft(), text("  x` `")]), "a b\n  x\\` `\n"),
        // A fence behind lost blanks takes its backslash; only the blanks are lost.
        (paragraph(&[text("/"), soft(), text("  ```")]), "/\n  \\```\n"),
        (paragraph(&[text("  ```` d"), soft(), text("c")]), "  \\```` d\nc\n"),
    ] {
        assert_eq!(write(&tree), (expected.to_string(), unspellable.clone()), "{expected:?}");
    }
}
