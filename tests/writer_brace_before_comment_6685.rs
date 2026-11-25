//! carve#6685, C2-W-088, C2-W-117: a paragraph whose first line is a whole attribute block keeps the backslash
//! of its brace before a line comment line, which the bare line would turn into an attribute line.

use carve::api::{Carve, Options, WriteOptions};
use serde_json::Value;

fn without_pos(v: &mut Value) {
    match v {
        Value::Object(m) => {
            m.remove("pos");
            m.remove("srcByteLength");
            m.values_mut().for_each(without_pos);
        }
        Value::Array(a) => a.iter_mut().for_each(without_pos),
        _ => {}
    }
}

fn shape(tree: &carve::api::Tree) -> Value {
    let mut v: Value = serde_json::from_str(&carve::api::encode_tree(tree)).expect("json");
    without_pos(&mut v);
    v
}

fn fmt(src: &str) -> (String, Vec<String>) {
    let carve = Carve::new();
    let written = carve
        .format_with_diagnostics(src, &WriteOptions::default())
        .expect("a format");
    let original = carve.parse(src, &Options::default()).expect("a parse");
    let read_back = carve
        .parse(&written.output, &Options::default())
        .expect("a parse");
    assert_eq!(shape(&read_back), shape(&original), "{src:?}");
    let codes = written
        .diagnostics
        .iter()
        .map(|e| e.code.to_string())
        .collect();
    (written.output, codes)
}

#[test]
fn a_leading_brace_keeps_its_backslash_before_a_line_comment() {
    for src in [
        "\\{a}\n%% c\n",
        "\\{#a .b k=v}\n%% c\n",
        "\\{a}\n%%\n",
        "\\{a}\n%% c\nx\n",
        "> \\{a}\n> %% c\n",
        "- \\{a}\n  %% c\n",
    ] {
        assert_eq!(fmt(src), (src.to_string(), Vec::new()), "{src:?}");
    }
}

#[test]
fn a_brace_inside_the_paragraph_stays_bare_before_a_line_comment() {
    let src = "y\n{a}\n%% c\n";
    assert_eq!(fmt(src), (src.to_string(), Vec::new()));
}
