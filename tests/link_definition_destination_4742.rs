use carve::api::{Carve, Options};

#[test]
fn definition_destinations_only_consume_parenthesis_and_backslash_escapes() {
    for (input, href) in [
        ("[a]: /u\\_v\n", "/u\\_v"),
        ("[a]: \\*\n", "\\*"),
        ("[a]: \\(\n", "("),
        ("[a]: a\\\\b\n", "a\\b"),
    ] {
        let tree = Carve::new().parse(input, &Options::default()).unwrap();
        let json: serde_json::Value =
            serde_json::from_str(&carve::api::encode_tree(&tree)).unwrap();
        assert_eq!(json["children"][0]["href"], href, "{input:?}");
    }
}

#[test]
fn definition_destination_variants_keep_other_escaped_punctuation() {
    let carve = Carve::new();
    let mut checked = 0;
    for punct in ['_', '*', '/', '-', '.', ':', '!', '?', '#', '%'] {
        for prefix in ["", "a", "/u", "dir/", "long/path"] {
            for suffix in ["", "a", "1", "v", "tail"] {
                for label in ["a", "ab", "ref", "x"] {
                    let href = format!("{prefix}\\{punct}{suffix}");
                    let input = format!("[{label}]: {href}\n");
                    let tree = carve.parse(&input, &Options::default()).unwrap();
                    let json: serde_json::Value =
                        serde_json::from_str(&carve::api::encode_tree(&tree)).unwrap();
                    assert_eq!(json["children"][0]["href"], href, "{input:?}");
                    checked += 1;
                }
            }
        }
    }
    assert_eq!(checked, 1_000);
}
