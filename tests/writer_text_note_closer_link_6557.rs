//! carve#6557, C2-W-125 case eight, C2-W-075: a text `^[` before a link whose text is a caret keeps its
//! place, its early closer takes the backslash and the link caret stays bare across a space or a line
//! break; a full or collapsed reference keeps its authored spelling.

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
fn the_link_caret_stays_bare_behind_an_early_note_closer() {
    for separator in ["\n", " "] {
        for (link, expected) in [
            ("[^](u)", "[^](u)"),
            ("[\\^](u)", "[^](u)"),
            ("[^][n]", "[^][n]"),
            ("[\\^][n]", "[\\^][n]"),
            ("[^][]", "[^][]"),
            ("[\\^][]", "[\\^][]"),
        ] {
            let src = format!("^[\\]{separator}{link}\n");
            let written = format!("^[\\]{separator}{expected}\n");
            assert_eq!(fmt(&src), (written.clone(), Vec::new()), "{src:?}");
            assert_eq!(fmt(&written), (written.clone(), Vec::new()), "{written:?}");
        }
    }
}

#[test]
fn every_early_closer_of_the_note_takes_the_backslash() {
    let src = "*^[\\]\\]\nx [\\^](u)*\n";
    assert_eq!(fmt(src), ("*^[\\]\\]\nx [^](u)*\n".to_string(), Vec::new()));
}

#[test]
fn only_the_first_caret_link_behind_the_closer_stays_bare() {
    let src = "^[\\] text [\\^](u) [\\^](v)\n";
    assert_eq!(
        fmt(src),
        ("^[\\] text [^](u) [\\^](v)\n".to_string(), Vec::new())
    );
}

#[test]
fn a_caret_link_that_reads_as_a_footnote_key_bare_keeps_its_backslash() {
    for src in [
        "^[\\] [^a](u) [^](u)\n",
        "^[\\] [^a](u) [\\^](u)\n",
        "^[\\]\\] [\\^][] ^[\\] [^a](u)\n",
    ] {
        let (written, codes) = fmt(src);
        assert!(codes.is_empty(), "{src:?}: {codes:?}");
        assert_eq!(fmt(&written).0, written, "{src:?}");
    }
}
