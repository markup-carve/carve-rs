//! markup-carve/carve#4469 (CARVE-P9-047), corpus rows
//! 855-a-comment-block-in-a-line-block-leaves-nothing.

fn children(source: &str) -> serde_json::Value {
    let mut doc: serde_json::Value =
        serde_json::from_str(&carve::to_json(&carve::parse(source))).unwrap();
    fn strip(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                map.remove("pos");
                map.values_mut().for_each(strip);
            }
            serde_json::Value::Array(items) => items.iter_mut().for_each(strip),
            _ => {}
        }
    }
    strip(&mut doc);
    doc["children"].clone()
}

#[test]
fn a_closed_pair_leaves_nothing() {
    for (source, html) in [
        (
            "::: |\n%%%\nsecret\n%%%\n:::\n",
            "<div class=\"line-block\">\n\n</div>",
        ),
        (
            "::: |\na\n%%%\none\n\ntwo\n%%%\nc\n:::\n",
            "<div class=\"line-block\">\n  <p>a<br>\nc</p>\n</div>",
        ),
        (
            "::: |\na\n%%%% t\nx\n  %%%%\nc\n:::\n",
            "<div class=\"line-block\">\n  <p>a<br>\nc</p>\n</div>",
        ),
        (
            "::: |\na\n%%%\nc\n:::\n",
            "<div class=\"line-block\">\n  <p>a<br>\n<br>\nc</p>\n</div>",
        ),
        (
            "::: |\n %%%\nx\n%%%\n:::\n",
            "<div class=\"line-block\">\n  <p>&nbsp;%%%<br>\nx<br>\n</p>\n</div>",
        ),
        (
            "::: |\na\n\n%%%\nx\n%%%\n\nb\n:::\n",
            "<div class=\"line-block\">\n  <p>a</p>\n  <p>b</p>\n</div>",
        ),
    ] {
        assert_eq!(carve::to_html(source), html, "{source:?}");
    }
}

#[test]
fn the_pair_is_a_block_comment_in_the_stanza() {
    let tree = children("::: |\na\n%%% t\nx\n%%%\nc\n:::\n");
    let kinds: Vec<&str> = tree[0]["children"][0]["children"]
        .as_array()
        .unwrap()
        .iter()
        .map(|node| node["type"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["text", "comment", "hard_break", "text"]);
    let comment = &tree[0]["children"][0]["children"][1];
    assert_eq!(comment["block"], true);
    assert_eq!(comment["content"], "t\nx");
}

#[test]
fn the_pair_is_written_back_unchanged() {
    for source in [
        "::: |\n%%%\nsecret\n%%%\n:::\n",
        "::: |\na\n%%%\none\n\ntwo\n%%%\nc\n:::\n",
        "::: |\n%%%\nx\n%%%\nc\n:::\n",
        "::: |\na\\\n%%%\nx\n%%%\n:::\n",
        "> ::: |\n> a\\\n> %%%\n> x\n> %%%\n> :::\n",
        "::: |\n*a\n%%%\nx\n%%%\nb*\n:::\n",
    ] {
        assert_eq!(carve::to_carve(source), source, "{source:?}");
    }
}

#[test]
fn the_written_fence_pairs_with_no_comment_line() {
    let source = "::: |\n%%% t\n%%%%\n  %%%%\na\n:::\n";
    let once = carve::to_carve(source);
    assert_eq!(children(&once), children(source));
    assert_eq!(
        carve::to_html(&once),
        "<div class=\"line-block\">\n  <p><br>\na</p>\n</div>"
    );
}
