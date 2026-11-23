//! carve-rs#4643 and #4644: a comment block in a line block opens with words
//! behind its run (C2-B-108), and an unclosed one ends with the line block and is
//! kept by fmt (C2-B-111).

use carve::api::{self, Carve, EncodeOptions, LintOptions, Options, WriteOptions};
use serde_json::{Value, json};

fn tree(source: &str) -> Value {
    let carve = Carve::new();
    let encoded = api::encode_tree_with(
        &carve.parse(source, &Options::default()).unwrap(),
        EncodeOptions { positions: false },
    );
    let mut value: Value = serde_json::from_str(&encoded).unwrap();
    value.as_object_mut().unwrap().remove("srcByteLength");
    value
}

fn fmt(source: &str) -> String {
    Carve::new()
        .format(source, &WriteOptions::default())
        .unwrap()
}

fn render(source: &str) -> String {
    Carve::new().render(source, &Options::default()).unwrap()
}

#[test]
fn c2_b_108_words_behind_the_opener_run_are_the_first_value_line() {
    let source = "- ::: |\n  %%% t\n  c\n  %%%\nz\n";
    let doc = tree(source);
    assert_eq!(
        doc["children"][0]["children"][0]["children"][0]["children"],
        json!([{"type": "comment", "form": "block", "value": "t\nc"}])
    );
    assert_eq!(
        render(source),
        "<ul>\n  <li>\n    <div class=\"line-block\">\n\n    </div>\n  </li>\n</ul>\n<p>z</p>"
    );
    let written = fmt(source);
    assert_eq!(written, "- ::: |\n  %%%\n  t\n  c\n  %%%\n  :::\n\nz\n");
    assert_eq!(tree(&written), doc);
    assert_eq!(fmt(&written), written);
}

#[test]
fn c2_b_108_opener_with_words_in_other_containers() {
    for (source, value) in [
        ("::: |\na\n%%% t\n%%%\n:::\n", "t"),
        ("::: |\n%%%%   t  u\nc\n%%%%\n:::\n", "t  u\nc"),
        ("> ::: |\n> %%%\tt\n> %%%\n> :::\n", "t"),
        ("1. ::: |\n   %%% t\n   %%%\n   :::\n", "t"),
    ] {
        let doc = tree(source);
        let expected = json!({"form": "block", "type": "comment", "value": value}).to_string();
        assert!(doc.to_string().contains(&expected), "{source:?}: {doc}");
        let written = fmt(source);
        assert_eq!(tree(&written), doc, "{source:?} -> {written:?}");
        assert_eq!(fmt(&written), written);
    }
}

#[test]
fn c2_b_111_an_unclosed_comment_block_is_kept() {
    let source = "::: |\na\n%%%\nc\n";
    assert_eq!(
        tree(source)["children"][0]["children"],
        json!([
            {"type": "line", "children": [{"type": "text", "value": "a"}]},
            {"type": "comment", "form": "block", "value": "c"},
        ])
    );
    let written = fmt(source);
    assert_eq!(written, "::: |\na\n%%%\nc\n%%%\n:::\n");
    assert_eq!(tree(&written), tree(source));
    assert_eq!(fmt(&written), written);
}

#[test]
fn c2_b_111_an_unclosed_comment_block_ends_with_its_container() {
    for source in ["- ::: |\n  %%% t\n  c\nz\n", "> ::: |\n> %%%\n> c\n\nz\n"] {
        let doc = tree(source);
        let text = doc.to_string();
        assert!(text.contains(r#""form":"block""#), "{source:?}: {text}");
        let written = fmt(source);
        assert_eq!(tree(&written), doc, "{source:?} -> {written:?}");
        assert_eq!(fmt(&written), written);
    }
}

/// C2-B-137 tests the closer before ownership; C2-B-024 lets only an open stanza with no
/// blank line held take a lazy line, here into the open comment block.
#[test]
fn unowned_lines_while_a_comment_block_is_open() {
    for (source, written) in [
        (
            "- ::: |\n  a\n  %%%\n  c\nz\n",
            "- ::: |\n  a\n  %%%\n  c\n  z\n  %%%\n  :::\n",
        ),
        (
            "- ::: |\n  a\n  %%%\n  c\n%% x\n",
            "- ::: |\n  a\n  %%%\n  c\n  %% x\n  %%%\n  :::\n",
        ),
        (
            "- ::: |\n  %%%\n%%%\nz\n",
            "- ::: |\n  %%%\n  %%%\n  :::\n\nz\n",
        ),
        (
            "- ::: |\n  a\n  %%% t\n\nz\n",
            "- ::: |\n  a\n  %%%\n  t\n  %%%\n  :::\n\nz\n",
        ),
    ] {
        assert_eq!(fmt(source), written, "{source:?}");
        assert_eq!(tree(written), tree(source), "{source:?}");
        assert_eq!(fmt(written), written);
    }
}

#[test]
fn an_unclosed_line_block_names_its_width_and_kind() {
    let entries = api::lint("::: |\na\n", &LintOptions::default()).unwrap();
    let entry = entries
        .iter()
        .find(|e| e.code == "unclosed-container-fence")
        .unwrap();
    assert!(
        entry
            .message
            .starts_with("This 3-colon line block has no closer"),
        "{}",
        entry.message
    );
}

/// markup-carve/carve#6439 rows A, B, C, E, F, G and H: the Expected section, which carve-js gives too.
#[test]
fn carve_6439_unowned_lines_and_closers_around_a_comment_block() {
    for (what, source, html) in [
        (
            "a column-0 line closes the item with the open comment",
            "- a\n  - ::: |\n    %%%\nb\n",
            "<ul>\n  <li>a\n    <ul>\n      <li>\n        <div class=\"line-block\">\n\n        </div>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>b</p>",
        ),
        (
            "no stanza takes a line after a closed comment",
            "- a\n  - ::: |\n    %%%\n    x\n    %%%\nb\n",
            "<ul>\n  <li>a\n    <ul>\n      <li>\n        <div class=\"line-block\">\n\n        </div>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>b</p>",
        ),
        (
            "a line without the quote marks closes the quotes",
            "> > ::: |\n> > %%%\ny\n",
            "<blockquote>\n  <blockquote>\n    <div class=\"line-block\">\n\n    </div>\n  </blockquote>\n</blockquote>\n<p>y</p>",
        ),
        (
            "a below-column closer closes the comment",
            "- - ::: |\n    %%%\n   %%%%\n    %%%\n    b\nz\n",
            "<ul>\n  <li>\n    <ul>\n      <li>\n        <div class=\"line-block\">\n\n        </div>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>z</p>",
        ),
        (
            "a closer at column two closes the comment",
            "- - ::: |\n    %%%\n  %%%%\n    %%%\n    b\nz\n",
            "<ul>\n  <li>\n    <ul>\n      <li>\n        <div class=\"line-block\">\n\n        </div>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>z</p>",
        ),
        (
            "a lazy closer keeps the open stanza",
            "- - ::: |\n    a\n    %%%\n%%%%\nz\n",
            "<ul>\n  <li>\n    <ul>\n      <li>\n        <div class=\"line-block\">\n          <p>a<br>\nz</p>\n        </div>\n      </li>\n    </ul>\n  </li>\n</ul>",
        ),
        (
            "an unowned line closes the item before a later opener",
            "- - ::: |\n    %%%\nx\n    %%%\nz\n",
            "<ul>\n  <li>\n    <ul>\n      <li>\n        <div class=\"line-block\">\n\n        </div>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>x</p>",
        ),
    ] {
        assert_eq!(render(source), html, "{what}: {source:?}");
        let written = fmt(source);
        assert_eq!(tree(&written), tree(source), "{what}: {written:?}");
        assert_eq!(fmt(&written), written);
    }
}
