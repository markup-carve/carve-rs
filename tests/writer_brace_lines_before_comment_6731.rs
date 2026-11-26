//! carve#6731, C2-I-028, C2-W-088: stacked whole attribute block lines before a line comment escape only the first
//! brace; the later lines are paragraph text behind it and report no kind.

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
fn only_the_first_of_stacked_brace_lines_takes_a_backslash() {
    for src in [
        "\\{a}\n{b}\n%% c\n",
        "\\{a}\n{b}\n{c}\n%% c\n",
        "\\{a}\n{b}\n%%\n",
        "\\{#a .b k=v}\n{.e}\n%% c\n\np\n",
        "> \\{a}\n> {b}\n> %% c\n",
        "- \\{a}\n  {b}\n  %% c\n",
    ] {
        assert_eq!(fmt(src), (src.to_string(), Vec::new()), "{src:?}");
    }
}

#[test]
fn an_escaped_later_brace_line_drops_its_backslash() {
    assert_eq!(
        fmt("\\{a}\n\\{b}\n\\{c}\n%% c\n"),
        ("\\{a}\n{b}\n{c}\n%% c\n".to_string(), Vec::new())
    );
}

#[test]
fn brace_lines_before_a_paragraph_line_keep_both_backslashes() {
    let src = "\\{a}\n\\{b}\nx\n";
    assert_eq!(fmt(src), (src.to_string(), Vec::new()));
}
