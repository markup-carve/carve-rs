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
fn line_block_text_that_starts_a_comment_fence_is_escaped() {
    for (source, expected) in [
        ("::: |\n\\%%%\n", "::: |\n\\%%%\n:::\n"),
        ("- ::: |\n  \\%%%a\n", "- ::: |\n  \\%%%a\n  :::\n"),
    ] {
        check(source, expected);
    }
}

#[test]
fn generated_line_opener_families_read_back() {
    // 125 suffixes across eight container variants.
    let containers = 8;
    for i in 0..125 {
        for index in 0..containers {
            let suffix = format!("a{i}x{index}");
            let line_source = if index % 2 == 0 {
                format!("::: |\n\\%%%{suffix}\n")
            } else {
                format!("- ::: |\n  \\%%%{suffix}\n")
            };
            let line_expected = if index % 2 == 0 {
                format!("::: |\n\\%%%{suffix}\n:::\n")
            } else {
                format!("- ::: |\n  \\%%%{suffix}\n  :::\n")
            };
            check(&line_source, &line_expected);
        }
    }
}
