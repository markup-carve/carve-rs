//! Runs of lazy `%%` lines below a line block line (carve#6472).

use carve::api::{self, Carve, EncodeOptions, Options, WriteOptions};
use serde_json::Value;

fn tree(source: &str) -> Value {
    let carve = Carve::new();
    let encoded = api::encode_tree_with(
        &carve.parse(source, &Options::default()).unwrap(),
        EncodeOptions { positions: false },
    );
    let mut value: Value = serde_json::from_str(&encoded).unwrap();
    value.as_object_mut().unwrap().remove("srcByteLength");
    value
}

fn fmt(source: &str) -> String {
    Carve::new()
        .format(source, &WriteOptions::default())
        .unwrap()
}

fn round_trips(source: &str) -> String {
    let written = fmt(source);
    assert_eq!(tree(&written), tree(source), "{source:?} -> {written:?}");
    assert_eq!(fmt(&written), written, "{written:?} is no fixed point");
    written
}

#[test]
fn c2_b_024_every_lazy_comment_below_a_line_is_written() {
    // carve#6472.
    let source = "- ::: |\n  a\n%% x\n%% y\n";
    assert_eq!(round_trips(source), "- ::: |\n  a\n%% x\n%% y\n  :::\n");
    for source in [
        "> ::: |\n> a\n%% x\n%% y\n%% z\n> b\n",
        "- ::: |\n  a\n%% x\n%%\n  b\n",
    ] {
        round_trips(source);
    }
}
