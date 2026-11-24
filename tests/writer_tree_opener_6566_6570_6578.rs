use carve::api::{Carve, Options, WriteOptions};

fn semantic_tree(tree: &carve::api::Tree) -> serde_json::Value {
    fn strip(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                for key in ["pos", "srcByteLength", "stages"] {
                    map.remove(key);
                }
                for child in map.values_mut() {
                    strip(child);
                }
            }
            serde_json::Value::Array(items) => {
                for item in items {
                    strip(item);
                }
            }
            _ => {}
        }
    }
    let mut value = serde_json::from_str(&carve::api::encode_tree(tree)).unwrap();
    strip(&mut value);
    value
}

fn check_writes(cases: &[(&str, &str)]) {
    let carve = Carve::new();
    let opts = WriteOptions::default();
    for &(source, expected) in cases {
        let written = carve.format_with_diagnostics(source, &opts).unwrap();
        assert_eq!(written.output, expected, "{source}");
        assert!(written.diagnostics.is_empty(), "{source}");
        let tree = carve.parse(source, &Options::default()).unwrap();
        let supplied = carve.write_with_diagnostics(&tree, &opts).unwrap();
        assert_eq!(supplied.output, expected, "{source}");
        let back = carve.parse(expected, &Options::default()).unwrap();
        assert_eq!(semantic_tree(&back), semantic_tree(&tree), "{source}");
        let again = carve.format_with_diagnostics(expected, &opts).unwrap();
        assert_eq!(again.output, expected, "{source}");
    }
}

// carve#6566, C2-W-088: a span on the opener of a node of the tree is that node; C2-W-125 repairs it and no kind
// is reported (CC-2018 to CC-2020).
#[test]
fn span_on_tree_opener_reports_no_kind() {
    check_writes(&[
        ("\\^^},} {,\\{,,}\\},}\\,{,\n", "^^},} {,\\{,,}},},{,\n"),
        ("- {^\\}^{a^\\}{^^}^},,,}\n", "- {^}^{a\\^}{^^}^},,,}\n"),
        (
            "- {^^}\\{\\},,{,\\^\\{,},}}}{,,}^{,,},}\n",
            "- {^^}{},,{,^\\{,},}}}{,,}^{,,},}\n",
        ),
        (
            "{^^}\\{\\},,{,\\^\\{,},}}}{,,}^{,,},} b\n",
            "{^^}{},,{,^\\{,},}}}{,,}^{,,},} b\n",
        ),
        ("> {,\\{,,}\\},}\\,{,\n", "> {,\\{,,}},},{,\n"),
    ]);
}

// carve#6570, C2-W-125 case nine (CC-2021): the `=` behind the key characters makes the attribute block valid.
#[test]
fn underline_read_as_attribute_block_escapes_equals() {
    check_writes(&[
        ("{_=\\._}\n", "{_\\=._}\n"),
        ("{_=\\!_}\n", "{_\\=!_}\n"),
        ("x{_=\\._}y\n", "x{_\\=._}y\n"),
        ("> a {_=\\._} b\n", "> a {_\\=._} b\n"),
        ("- {_=\\._}\n", "- {_\\=._}\n"),
        ("| {_=\\._} |\n", "| {_\\=._} |\n"),
        ("{_foo-bar=\\._}\n", "{_foo-bar\\=._}\n"),
        ("{_a\\=b_}\n", "{_a\\=b_}\n"),
    ]);
}
