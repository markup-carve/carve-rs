//! carve#6308: an attribute line of a list after an ordered-list style change
//! keeps one blank line above it where the list before ends in an open paragraph
//! (C2-W-005, C2-B-024). CC-1480 to CC-1484 run in `writer_cases.rs`.

use carve::api::{self, Carve, EncodeOptions, Options, WriteOptions};
use serde_json::Value;

fn bare_tree(src: &str) -> Value {
    let tree = api::parse(src, &Options::default()).unwrap_or_else(|e| panic!("{src:?}: {e}"));
    let mut v: Value = serde_json::from_str(&api::encode_tree_with(
        &tree,
        EncodeOptions { positions: false },
    ))
    .unwrap_or_default();
    strip(&mut v);
    v
}

fn strip(v: &mut Value) {
    match v {
        Value::Object(m) => {
            m.remove("pos");
            m.remove("segments");
            m.remove("srcByteLength");
            m.values_mut().for_each(strip);
        }
        Value::Array(a) => a.iter_mut().for_each(strip),
        _ => {}
    }
}

#[test]
fn opaque_block_before_keeps_the_zero_blank_boundary() {
    let carve = Carve::new();
    let source = "v. ```\n   x\n   ```\n\n{x}\nvi. y\n";
    assert_eq!(
        carve.format(source, &WriteOptions::default()).unwrap(),
        "v. ```\n   x\n   ```\n{x}\nvi. y\n"
    );
}

#[test]
fn attribute_lines_at_style_boundaries_round_trip() {
    let carve = Carve::new();
    let opts = WriteOptions::default();
    let first = ["v. x", "a. x", "i. x", "A. x", "V. x", "1. x"];
    let second = ["vi. y", "1. y", "b. y", "I. y", "2. y"];
    let attrs = ["{x}", "{#i}", "{.c}", "{k=v}", "{#i .c}", "{start=7}"];
    let prefixes = ["", "> ", "- q\n\n  "];
    let contents = ["y", "[y](u)"];
    let mut failures = Vec::new();
    for at in 0..1080 {
        let left = first[at / 180];
        let marker = second[(at / 36) % second.len()];
        let attr = attrs[(at / 6) % attrs.len()];
        let prefix = prefixes[(at / 2) % prefixes.len()];
        let right = marker.replace(" y", &format!(" {}", contents[at % contents.len()]));
        let input = match prefix {
            "> " => format!("> {left}\n>\n> {attr}\n> {right}\n"),
            "" => format!("{left}\n\n{attr}\n{right}\n"),
            _ => format!("- q\n\n  {left}\n\n  {attr}\n  {right}\n"),
        };
        let once = match carve.format(&input, &opts) {
            Ok(once) => once,
            Err(e) => {
                failures.push(format!("{input:?}: {e}"));
                continue;
            }
        };
        if bare_tree(&once) != bare_tree(&input) {
            failures.push(format!("{input:?} -> {once:?}: another tree"));
        }
        match carve.format(&once, &opts) {
            Ok(twice) if twice == once => {}
            other => failures.push(format!("{input:?} -> {once:?} -> {other:?}")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
