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

fn check(source: &str, expected: &str) {
    let written = Carve::new()
        .format(source, &WriteOptions::default())
        .unwrap();
    assert_eq!(written, expected, "{source:?}");
    assert_eq!(tree(&written), tree(source), "{source:?}");
}

#[test]
fn a_paragraph_below_an_escaped_block_opener_takes_no_extra_escape() {
    for source in [
        "\\:::\n(\n",
        "\\%%\n<\n",
        "\\# #\n]\n",
        ":\n\\:::\n|\n",
        "\\%%\n*\n",
        "\\= )\n+\n",
        "\\:::\n{\n",
        "\\# a\n`\n",
        "\\[a]: `\n|\n",
    ] {
        let expected = if source == ":\n\\:::\n|\n" {
            "\\:\n\\:::\n|\n"
        } else {
            source
        };
        check(source, expected);
    }
}

#[test]
fn generated_line_opener_families_read_back() {
    // 125 suffixes across eight punctuation shapes.
    let following = ["(", "<", "]", "*", "+", "{", "`", "|"];
    for i in 0..125 {
        for (index, mark) in following.iter().enumerate() {
            let suffix = format!("a{i}x{index}");
            let paragraph = format!("\\:::\n{mark}{suffix}\n");
            check(&paragraph, &paragraph);
        }
    }
}
