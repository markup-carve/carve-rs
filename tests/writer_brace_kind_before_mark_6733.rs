//! carve#6733, carve#6559 shape 2: with kind `{` reported, the first `{` of every text `{` run takes the backslash
//! whatever mark follows it, and the escape stays (C2-W-088, C2-W-091).

use carve::api::{Carve, Options, WriteOptions};

fn semantic_tree(tree: &carve::api::Tree) -> serde_json::Value {
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
    let mut value = serde_json::from_str(&carve::api::encode_tree(tree)).unwrap();
    strip(&mut value);
    value
}

#[test]
fn a_text_brace_before_a_strong_or_emphasis_mark_keeps_the_kind_escape() {
    let carve = Carve::new();
    let opts = WriteOptions::default();
    for (source, expected) in [
        (
            "1. \\`^{/\\`c`c`\\{a}\\` {**}/}\n",
            "1. \\`^/\\`c`c`\\{a}\\` \\{**}/\n",
        ),
        ("`c`\\{a} {*x\n", "`c`\\{a} \\{*x\n"),
        ("`c`\\{a} {/x\n", "`c`\\{a} \\{/x\n"),
        ("`c`\\{a}{**x\n", "`c`\\{a}\\{**x\n"),
        ("> `c`\\{a} x{/y\n", "> `c`\\{a} x\\{/y\n"),
    ] {
        let written = carve.format_with_diagnostics(source, &opts).unwrap();
        assert_eq!(written.output, expected, "{source:?}");
        assert!(written.diagnostics.is_empty(), "{source:?}");
        let tree = carve.parse(source, &Options::default()).unwrap();
        let back = carve.parse(expected, &Options::default()).unwrap();
        assert_eq!(semantic_tree(&back), semantic_tree(&tree), "{source:?}");
        let again = carve.format_with_diagnostics(expected, &opts).unwrap();
        assert_eq!(again.output, expected, "{source:?}");
    }
}
