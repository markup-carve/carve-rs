//! carve#5502: a losing duplicate `+` line is read as if it were not there, so the
//! block the winning `+` takes stays in its note however many losing lines precede it
//! (PART 9R R2, the carve#5474 ruling). Expected outputs are the spec model's.

use carve::{to_html, to_json_with_options, Options};

fn check(source: &str, expected: &str) {
    assert_eq!(to_html(source).trim_end(), expected, "{source:?}");
}

#[test]
fn three_root_duplicates_keep_the_block_below_the_blank_in_the_note() {
    check(
        "[^m]: +\n[^m]: +\n[^m]: +\n\n  - item\n\nsee[^m]\n",
        "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <ul>\n        <li>item</li>\n      </ul>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
    );
}

#[test]
fn two_root_duplicates_keep_the_block_below_the_blank_in_the_note() {
    check(
        "[^m]: +\n[^m]: +\n\n  - item\n\nsee[^m]\n",
        "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <ul>\n        <li>item</li>\n      </ul>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
    );
}

#[test]
fn a_losing_line_in_a_quote_keeps_the_block_in_the_note() {
    check(
        "> [^m]: +\n> [^m]: +\n>\n>   - item\n\nsee[^m]\n",
        "<blockquote>\n\n</blockquote>\n<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <ul>\n        <li>item</li>\n      </ul>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
    );
}

#[test]
fn a_quoted_losing_line_hands_its_indented_follower_to_the_note() {
    check(
        "> [^m]: +\n> [^m]: +\n>   para\n\nsee[^m]\n",
        "<blockquote>\n\n</blockquote>\n<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>para<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
    );
}

#[test]
fn a_root_losing_line_hands_its_indented_follower_to_the_note() {
    check(
        "[^m]: +\n[^m]: +\n  para\n\nsee[^m]\n",
        "<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>para<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
    );
}

#[test]
fn four_duplicates_in_a_list_item_keep_the_paragraph_in_the_note() {
    check(
        "- [^m]: +\n  [^m]: +\n  [^m]: +\n  [^m]: +\n\n    para\n\nsee[^m]\n",
        "<ul>\n  <li></li>\n</ul>\n<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>para<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
    );
}

#[test]
fn a_description_note_keeps_the_next_description_4089() {
    check(
        ":: term\n: [^a]: +\n  flusha\n: next\n\nsee[^a]\n",
        "<dl>\n  <dt>term</dt>\n  <dd></dd>\n  <dd>next</dd>\n</dl>\n<p>see<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>flusha<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
    );
}

#[test]
fn the_positioned_tree_keeps_the_block_in_the_note_too() {
    let json = to_json_with_options(
        "[^m]: +\n[^m]: +\n[^m]: +\n\n  - item\n\nsee[^m]\n",
        &Options::default().with_positions(true),
    );
    let doc: serde_json::Value = serde_json::from_str(&json).expect("valid AST JSON");
    let children = doc["children"].as_array().expect("children");
    assert!(children.iter().all(|node| node["type"] != "list"), "{json}");
    let note = children
        .iter()
        .find(|node| node["type"] == "footnote")
        .expect("note");
    assert_eq!(note["children"][0]["type"], "list", "{json}");
}
