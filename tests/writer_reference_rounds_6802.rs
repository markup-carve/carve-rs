//! carve#6802: past the inline depth cap the rounds write each reference from the tree, take the verbatim texts
//! after them, and report every construct the parse opens on tree text (C2-W-088, C2-W-075).
use carve::api::{Carve, Options, WriteOptions};

fn semantic(source: &str) -> serde_json::Value {
    fn strip(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                for key in ["pos", "srcByteLength", "stages"] {
                    map.remove(key);
                }
                map.values_mut().for_each(strip);
            }
            serde_json::Value::Array(items) => items.iter_mut().for_each(strip),
            _ => {}
        }
    }
    let tree = Carve::new().parse(source, &Options::default()).unwrap();
    let mut value = serde_json::from_str(&carve::api::encode_tree(&tree)).unwrap();
    strip(&mut value);
    value
}

fn format(source: &str) -> String {
    let written = Carve::new()
        .format_with_diagnostics(source, &WriteOptions::default())
        .unwrap();
    assert!(written.diagnostics.is_empty(), "{:?}", written.diagnostics);
    written.output
}

#[test]
fn references_past_the_cap_are_written() {
    let source = "[a *[a][x]*[x]: /u\n".repeat(122);
    let out = format(&source);
    assert_eq!(
        out,
        include_str!("fixtures/writer_reference_rounds_6802.crv")
    );
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines[99], r"[a \*\[a][x]\*\[x\]: /u");
    assert_eq!(lines[100], r"\[a \*\[a][x]*\[x\]: /u");
    assert_eq!(semantic(&out), semantic(&source));
    assert_eq!(format(&out), out);
}

#[test]
fn references_past_the_cap_in_a_quote_are_written() {
    let source = "> [a *[a][x]*[x]: /u\n".repeat(122);
    let out = format(&source);
    assert!(out.lines().all(|line| line.starts_with("> ")), "{out}");
    assert_eq!(semantic(&out), semantic(&source));
    assert_eq!(format(&out), out);
}

#[test]
fn a_verbatim_reference_keeps_its_spelling() {
    let source = "a [b\\.c][x] and ![d\\_e][x]\n\n[x]: /u\n";
    assert_eq!(format(source), source);
}
