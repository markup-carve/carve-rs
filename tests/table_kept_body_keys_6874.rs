//! C2-W-136 (carve#6874): beside a body key kept in `attrs` the writer makes none of the three.
use carve::api::{self, Carve, Options, WriteOptions};
use serde_json::{Value, json};

fn written(tree: &Value) -> (String, Vec<(String, Option<String>)>) {
    let tree = api::decode_tree(&tree.to_string()).unwrap();
    let w = Carve::new()
        .write_with_diagnostics(&tree, &WriteOptions::default())
        .unwrap();
    let codes = w
        .diagnostics
        .into_iter()
        .map(|d| (d.code, d.path))
        .collect();
    (w.output, codes)
}

fn parsed(source: &str) -> Value {
    let tree = Carve::new().parse(source, &Options::default()).unwrap();
    let opts = api::EncodeOptions { positions: false };
    serde_json::from_str(&api::encode_tree_with(&tree, opts)).unwrap()
}

#[test]
fn a_tree_with_no_spelling_beside_a_kept_key_reports_only_that_key() {
    let mut tree = parsed("{body-rows=1,1}\n| a |\n| b |\n");
    tree["children"][0]["attrs"] = json!({"keyValues": [["body-header-rows", "x"]]});
    let (output, codes) = written(&tree);
    assert_eq!(output, "{body-rows=1,1}\n| a |\n| b |\n");
    assert_eq!(
        codes,
        [(
            "field-unspellable".to_string(),
            Some("/children/0/attrs/keyValues/0".to_string())
        )]
    );
}

#[test]
fn a_kept_key_keeps_the_marks_of_a_body_header_row() {
    for (source, expected) in [
        (
            "{header-rows=0 body-header-rows=1}\n|= h |\n| 1 |\n",
            "{body-header-rows=1 header-rows=0}\n|= h |\n| 1 |\n",
        ),
        (
            "{header-rows=1 body-rows=x}\n| d |\n|= a |\n",
            "{body-rows=x header-rows=1}\n| d |\n|= a |\n",
        ),
    ] {
        let (output, codes) = written(&parsed(source));
        assert_eq!(output, expected);
        assert!(codes.is_empty(), "{codes:?}");
        assert_eq!(parsed(&output)["children"], parsed(source)["children"]);
    }
}
