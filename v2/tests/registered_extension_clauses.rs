use carve2::reader::{TreeError, read_tree_value};
use serde::Deserialize as _;
use serde_json::Value;

#[test]
fn c2_x_031_038_046_registered_extension_refusals_name_the_node() {
    let text = include_str!("../spec-conformance/clause-cases.json");
    let mut parser = serde_json::Deserializer::from_str(text);
    parser.disable_recursion_limit();
    let corpus = Value::deserialize(&mut parser).expect("clause corpus");
    for id in ["CC-468", "CC-469", "CC-470", "CC-471", "CC-472", "CC-474"] {
        let case = corpus["cases"]
            .as_array()
            .expect("cases")
            .iter()
            .find(|case| case["id"] == id)
            .expect("clause case");
        let expected = case["expected"]["refusal"]["path"]
            .as_str()
            .expect("refusal path");
        let result = read_tree_value(case["tree"].clone());
        assert!(
            matches!(&result, Err(TreeError::Reader { path, .. } | TreeError::Schema { path, .. }) if path.starts_with(expected)),
            "{id}: {result:?}"
        );
    }
}
