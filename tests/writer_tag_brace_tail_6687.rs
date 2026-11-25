//! carve#6687, C2-W-125 case six, C2-W-088: once another node follows the brace around a tag tail, the case-six
//! backslash behind the name gives the tree alone, so neither brace takes a backslash.

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
fn a_brace_around_a_tag_tail_stays_bare_beside_another_node() {
    for src in [
        "x{#a\\_--} *s*\n",
        "x{#a\\_--} [l](u)\n",
        "x{#a\\_---} \"q\"\n",
        "x{#a\\-w}\n",
        "x{#a\\-w} *s*\n",
        "x{#a\\--w} `c`\n",
        "x{#a\\_--} w0\n",
    ] {
        assert_eq!(fmt(src), (src.to_string(), Vec::new()), "{src:?}");
    }
}

#[test]
fn a_brace_escape_stays_where_a_hyphen_run_or_another_escape_takes_one() {
    for src in ["x\\{#a\\---}\n", "x\\{#a\\-w} \\*s*\n"] {
        assert_eq!(fmt(src), (src.to_string(), Vec::new()), "{src:?}");
    }
}
