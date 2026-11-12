//! markup-carve/carve#5178 (CARVE-P0-005): an erased `%%` line is a child of
//! the innermost open container whose content column it reaches, else the
//! document; the lines after it do not move it.

use serde_json::Value;

fn tree(source: &str) -> Value {
    let options = carve::Options {
        positions: true,
        ..Default::default()
    };
    serde_json::from_str(&carve::to_json_with_options(source, &options)).unwrap()
}

/// The JSON path of the node holding the first comment, and its `pos`.
fn comment_parent(node: &Value, path: &str) -> Option<(String, Value)> {
    let fields = node.as_object()?;
    for (key, value) in fields {
        let Some(children) = value.as_array() else {
            continue;
        };
        for (i, child) in children.iter().enumerate() {
            if child["type"] == "comment" {
                return Some((path.to_string(), node["pos"].clone()));
            }
            if let Some(found) = comment_parent(child, &format!("{path}.{key}[{i}]")) {
                return Some(found);
            }
        }
    }
    None
}

fn span(pos: &Value) -> Option<(u64, u64)> {
    Some((pos["startOffset"].as_u64()?, pos["endOffset"].as_u64()?))
}

fn parent(source: &str) -> (String, Option<(u64, u64)>) {
    let (path, pos) = comment_parent(&tree(source), "$").expect("a comment");
    (path, span(&pos))
}

fn unchanged(source: &str) {
    let written = carve::to_carve(source);
    assert_eq!(written, source, "fmt of {source:?}");
    assert_eq!(carve::to_html(&written), carve::to_html(source));
}

#[test]
fn the_simple_case_is_the_items() {
    let source = "- a\n\n  %% n\n- b\n";
    assert_eq!(
        parent(source),
        ("$.children[0].items[0]".into(), Some((0, 11)))
    );
}

#[test]
fn corpus_620_14_is_the_outer_items() {
    let source = "- - a\n\n  %% just a note\n- b\n";
    assert_eq!(
        parent(source),
        ("$.children[0].items[0]".into(), Some((0, 23)))
    );
}

#[test]
fn corpus_504_18_is_the_descriptions() {
    let source = ":: a\n: b\n  :: c\n  %% note\n  more\n";
    assert_eq!(
        parent(source),
        ("$.children[0].items[1]".into(), Some((5, 32)))
    );
}

#[test]
fn corpus_937_is_the_descriptions_and_every_ancestor_reaches_it() {
    let source = "- :: term\n  : payload\n\n    %% c\n- sub\n";
    assert_eq!(
        parent(source),
        (
            "$.children[0].items[0].children[0].items[1]".into(),
            Some((12, 31))
        )
    );
    let doc = tree(source);
    let list = &doc["children"][0];
    assert_eq!(span(&list["pos"]), Some((0, 37)));
    assert_eq!(span(&list["items"][0]["pos"]), Some((0, 31)));
    assert_eq!(span(&list["items"][0]["children"][0]["pos"]), Some((2, 31)));
    assert_eq!(list["tight"], false);
    unchanged(&carve::to_carve(source));
}

#[test]
fn a_line_below_every_column_is_the_documents_whatever_follows() {
    for source in [
        "- a\n%% n\ntail\n",
        "> a\n%% n\n- b\n",
        "- - a\n%% n\nmore\n",
    ] {
        assert_eq!(parent(source).0, "$", "{source:?}");
    }
}

#[test]
fn a_line_at_an_outer_column_skips_the_inner_container_it_misses() {
    assert_eq!(
        parent("- x0\n  - x1\n  %% n\n  more\n").0,
        "$.children[0].items[0]"
    );
    assert_eq!(
        parent("::: note\n- x1\n  1. x2\n %% n\n\n:::\n").0,
        "$.children[0]"
    );
    assert_eq!(
        parent("1. x0\n   - x1\n    %% n\n    more\n").0,
        "$.children[0].items[0]"
    );
}

#[test]
fn the_writer_keeps_each_line_where_it_was_authored() {
    for source in [
        "- a\n%% n\ntail\n",
        "> a\n%% n\n> tail\n",
        "- - a\n  %% n\n  more\n",
        "- x0\n  - x1\n  %% n\n  more\n",
    ] {
        unchanged(source);
    }
}

/// markup-carve/carve#6003: a deeper line after the comment does not move it
/// into the container that line reaches.
#[test]
fn a_deeper_line_after_it_does_not_move_it() {
    let source = "- [^n]: t\n  %%\n\n    ~~~\n    e\n]\n";
    assert_eq!(
        parent(source),
        ("$.children[0].items[0]".into(), Some((0, 14)))
    );
    let doc = tree(source);
    assert_eq!(span(&doc["children"][0]["pos"]), Some((0, 14)));
    let note = &doc["children"][2];
    assert_eq!(note["type"], "footnote");
    assert_eq!(span(&note["pos"]), Some((2, 29)));
    assert_eq!(note["children"][1]["type"], "code_block");
    let written = carve::to_carve(source);
    assert_eq!(written, "- %%\n\n]\n\n[^n]: t\n\n  ```\n  e\n  ```\n");
    assert_eq!(carve::to_html(&written), carve::to_html(source));

    let source = "- - a\n\n  %% n\n\n    b\n";
    assert_eq!(
        parent(source),
        ("$.children[0].items[0]".into(), Some((0, 20)))
    );
    assert_eq!(
        tree(source)["children"][0]["items"][0]["children"][1]["type"],
        "comment"
    );
    let written = carve::to_carve(source);
    assert_eq!(parent(&written).0, "$.children[0].items[0]", "{written:?}");
    assert_eq!(carve::to_html(&written), carve::to_html(source));

    for (source, expected) in [
        (
            "- [^n]: t\n  %% n\n\n    deeper\n",
            "$.children[0].items[0]",
        ),
        ("> - a\n>\n> %% n\n>\n>   b\n", "$.children[0]"),
        ("- a\n\n%% n\n\n  b\n", "$"),
    ] {
        assert_eq!(parent(source).0, expected, "{source:?}");
    }
}
