//! carve#6470, C2-B-197: the bracket of a nested `![` opens a pair in the block image line, as in the inline
//! scan; `![^]` is no footnote reference (C2-I-087).

use serde_json::Value;

fn html(src: &str) -> String {
    carve::render(src, &carve::Options::default())
}

fn first_leaf(tree: &Value) -> &Value {
    let mut node = &tree["children"][0];
    while node["type"] != "paragraph" {
        node = &node["children"][0];
    }
    node
}

#[test]
fn a_nested_image_opener_pushes_a_bracket() {
    assert_eq!(html("![[![^]]](u)\n"), "<img src=\"u\" alt=\"[![^]]\">");
    assert_eq!(html("![![^ ]](u)\n"), "<img src=\"u\" alt=\"![^ ]\">");
}

/// The role is `image` exactly where the paragraph's inline content is one image.
#[test]
fn block_image_role_matches_the_inline_parse_around_a_nested_image() {
    let lines = [
        "![[![^]]](u)",
        "![![^ ]](u)",
        "1. ![[[[![^]][`[`][^x]][k]](u \"t\")",
        "> ![![^]](u)",
        "- ![a ![^][k]](u)",
        "![![^](u)",
    ];
    for line in lines {
        let src = format!("{line}\n\n[k]: /u\n\n[^x]: x\n");
        let tree: Value =
            serde_json::from_str(&carve::tree_json(&src, &carve::Options::default())).unwrap();
        let para = first_leaf(&tree);
        let kids = para["children"].as_array().unwrap();
        let only = kids.len() == 1 && kids[0]["type"] == "image";
        let role = para.get("role").and_then(Value::as_str) == Some("image");
        assert_eq!(only, role, "{line:?}: {para}");
    }
}
