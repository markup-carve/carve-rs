//! carve#6439 row D, Reading 1. C2-B-137: a comment-only line that ends an
//! opaque body closes only the container of that body.

use carve::api::{self, Carve, EncodeOptions, Options, WriteOptions};
use serde_json::Value;

fn tree(source: &str) -> Value {
    let carve = Carve::new();
    let encoded = api::encode_tree_with(
        &carve.parse(source, &Options::default()).unwrap(),
        EncodeOptions { positions: false },
    );
    serde_json::from_str(&encoded).unwrap()
}

fn types(nodes: &Value) -> Vec<String> {
    nodes
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["type"].as_str().unwrap().to_string())
        .collect()
}

fn render(source: &str) -> String {
    Carve::new().render(source, &Options::default()).unwrap()
}

fn fmt(source: &str) -> String {
    Carve::new()
        .format(source, &WriteOptions::default())
        .unwrap()
}

const ROW_D: &str = "- x\n  - ::: |\n    %%%\n%% c\n    %%%\nz\n";

#[test]
fn row_d_keeps_the_outer_item_open() {
    assert_eq!(
        render(ROW_D),
        "<ul>\n  <li>x\n    <ul>\n      <li>\n        <div class=\"line-block\">\n\n        </div>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>z</p>"
    );
    let doc = tree(ROW_D);
    assert_eq!(
        types(&doc["children"][0]["children"][0]["children"]),
        ["paragraph", "list", "comment", "comment"]
    );
}

#[test]
fn a_line_past_the_outer_marker_stays_in_the_outer_item_for_a_code_fence() {
    for source in ["- x\n  - ```\n%% c\n  y\n", "- x\n  - ```\n%% c\n    y\n"] {
        assert_eq!(
            render(source),
            "<ul>\n  <li>x\n    <ul>\n      <li>\n        <pre><code></code></pre>\n      </li>\n    </ul>\n    <p>y</p>\n  </li>\n</ul>",
            "{source:?}"
        );
    }
    let source = "- x\n  - %%%\n%% c\n  y\n";
    assert_eq!(
        types(&tree(source)["children"][0]["children"][0]["children"]),
        ["paragraph", "list", "comment", "paragraph"]
    );
}

#[test]
fn the_quote_around_the_item_stays_open() {
    assert_eq!(
        render("> - ```\n%% c\n> y\n"),
        "<blockquote>\n  <ul>\n    <li>\n      <pre><code></code></pre>\n    </li>\n  </ul>\n  <p>y</p>\n</blockquote>"
    );
}

#[test]
fn a_description_list_ends_with_its_description() {
    assert_eq!(
        types(&tree(":: t\n: ```\n%% c\n")["children"]),
        ["definition_list", "comment"]
    );
    assert_eq!(
        types(&tree(":: t\n: ```\n%% c\n: d\n")["children"]),
        ["definition_list", "comment", "paragraph"]
    );
}

#[test]
fn a_region_marker_line_still_closes_every_unowned_container() {
    assert_eq!(
        render("- x\n  - ```\n%% #region a\n  y\n"),
        "<ul>\n  <li>x\n    <ul>\n      <li>\n        <pre><code></code></pre>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>y</p>"
    );
}

#[test]
fn the_code_fence_forms_write_back_stable() {
    for source in ["- x\n  - ```\n%% c\n  y\n", "> - ```\n%% c\n> y\n"] {
        let written = fmt(source);
        assert_eq!(
            render(&written),
            render(source),
            "{source:?} -> {written:?}"
        );
        assert_eq!(fmt(&written), written);
    }
}
