use carve::api::{self, Options};

#[test]
fn footnote_closer_follows_the_full_reference_label() {
    for input in ["^[^[b][[c]]\n", "x ^[a ^[b][[c] d] y\n", "> ^[^[b][[c]]\n"] {
        let tree = api::parse(input, &Options::default()).expect("parse");
        let text: serde_json::Value =
            serde_json::from_str(&api::encode_tree(&tree)).expect("tree value");
        let paragraph = if input.starts_with('>') {
            &text["children"][0]["children"][0]
        } else {
            &text["children"][0]
        };
        assert!(
            paragraph["children"]
                .as_array()
                .is_some_and(|kids| kids.iter().any(|n| n["type"] == "inline_footnote")),
            "{input}: {text}"
        );
        if input == "^[^[b][[c]]\n" {
            let note = &paragraph["children"][0];
            assert_eq!(note["type"], "inline_footnote", "{text}");
            assert_eq!(note["children"][0]["value"], "^", "{text}");
            assert_eq!(note["children"][1]["type"], "link", "{text}");
            assert_eq!(note["children"][1]["key"], "[c", "{text}");
        }
    }
}

#[test]
fn full_reference_label_stays_inside_the_note_across_1000_surroundings() {
    fn notes_with_key(node: &serde_json::Value, key: &str) -> usize {
        let here = usize::from(
            node["type"] == "inline_footnote"
                && node["children"].as_array().is_some_and(|children| {
                    children
                        .iter()
                        .any(|child| child["type"] == "link" && child["key"] == key)
                }),
        );
        here + node["children"].as_array().map_or(0, |children| {
            children.iter().map(|c| notes_with_key(c, key)).sum()
        })
    }

    let mut count = 0;
    for container in ["", "> ", "- ", "1. "] {
        for label in ["b", "bb", "word", "item2", "alpha"] {
            for target in ["c", "cc", "ref", "item3", "beta"] {
                for prefix in ["", "before ", "x ", "left: ", "z "] {
                    for suffix in ["", " tail"] {
                        let input =
                            format!("{container}{prefix}^[^[{label}][[{target}]]{suffix}\n");
                        let tree = api::parse(&input, &Options::default()).expect("parse");
                        let json: serde_json::Value =
                            serde_json::from_str(&api::encode_tree(&tree)).expect("tree value");
                        assert_eq!(notes_with_key(&json, &format!("[{target}")), 1, "{input}");
                        count += 1;
                    }
                }
            }
        }
    }
    assert_eq!(count, 1000);
}
