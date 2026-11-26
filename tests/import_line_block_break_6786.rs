//! C2-H-195 row 17 (carve#6786, Ruled A) and C2-H-043 (carve#6785): the hard
//! break of a line-block line on import. The expected sources and trees are
//! CC-2167 to CC-2171 of carve next2 307e80eb and the Expected of carve#6785.
mod common;

use carve::Options;
use carve::api::{self, ImportOptions};
use common::parse_json;
use serde_json::{Value, json};

fn import(html: &str) -> (String, Value) {
    let got = api::import_document(html, &ImportOptions::default()).expect("import");
    let tree = parse_json(&api::encode_tree_with(
        &got.tree,
        api::EncodeOptions { positions: false },
    ));
    (got.source, tree["children"].clone())
}

fn block(html: &str) -> String {
    format!("<div class=\"line-block\" data-carve-line-block>\n  {html}\n</div>")
}

fn text(v: &str) -> Value {
    json!({ "type": "text", "value": v })
}

fn line(children: &[Value]) -> Value {
    json!({ "type": "line", "children": children })
}

fn hard() -> Value {
    json!({ "type": "hard_break" })
}

fn lines(children: &[Value]) -> Value {
    json!([{ "type": "line_block", "children": children }])
}

#[test]
fn hard_break_kept_where_the_line_needs_it() {
    let b = text("b");
    let cases = [
        // CC-2167
        (
            "<p>a <br>\nb</p>",
            "::: |\na \\\nb\n:::\n",
            lines(&[line(&[text("a "), hard()]), line(&[b.clone()])]),
        ),
        // CC-2168
        (
            "<p>a<br>\n<br>\nb</p>",
            "::: |\na\n\\\nb\n:::\n",
            lines(&[line(&[text("a")]), line(&[hard()]), line(&[b.clone()])]),
        ),
        // CC-2169
        (
            "<p>a <code>x</code> <br>\nb</p>",
            "::: |\na `x` \\\nb\n:::\n",
            lines(&[
                line(&[
                    text("a "),
                    json!({ "type": "code", "value": "x" }),
                    text(" "),
                    hard(),
                ]),
                line(&[b.clone()]),
            ]),
        ),
        // CC-2170
        (
            "<p>a&nbsp;&nbsp;&nbsp;<br>\nb</p>",
            "::: |\na   \\\nb\n:::\n",
            lines(&[line(&[text("a   "), hard()]), line(&[b.clone()])]),
        ),
        // CC-2171: the ordinary line of row 18 still drops it.
        (
            "<p>a<br>\nb</p>",
            "::: |\na\nb\n:::\n",
            lines(&[line(&[text("a")]), line(&[b.clone()])]),
        ),
    ];
    for (html, carve, tree) in cases {
        let (source, children) = import(&block(html));
        assert_eq!(source, carve, "{html}");
        assert_eq!(children, tree, "{html}");
    }
}

#[test]
fn final_br_ends_the_last_line_of_its_stanza() {
    // CC-160
    let (source, children) = import(&block("<p>a<br>\n</p>\n  <p>b</p>"));
    assert_eq!(source, "::: |\na\\\n\nb\n:::\n");
    assert_eq!(
        children,
        lines(&[line(&[text("a"), hard()]), line(&[]), line(&[text("b")])])
    );
    let (source, children) = import(&block("<p>a<br>\n</p>"));
    assert_eq!(source, "::: |\na\\\n:::\n");
    assert_eq!(children, lines(&[line(&[text("a"), hard()])]));
}

#[test]
fn line_block_breaks_round_trip_through_import() {
    let options = Options {
        roundtrip: true,
        ..Options::default()
    };
    for src in [
        "::: |\na \\\nb\n:::\n",
        "::: |\na\n\\\nb\n:::\n",
        "::: |\na\\\n\nb\n:::\n",
        "::: |\na\\\n:::\n",
        "::: |\na\n\\\n:::\n",
        "::: |\na\t\\\nb `c` \\\n\\\n:::\n",
    ] {
        let html = api::render(src, &options).expect("render");
        let (source, _) = import(&html);
        let again = api::render(source.as_str(), &options).expect("render");
        assert_eq!(again, html, "{src:?} imported as {source:?}");
    }
}
