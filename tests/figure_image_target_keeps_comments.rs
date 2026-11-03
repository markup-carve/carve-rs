//! CARVE-P12-036: a captioned image paragraph retains every inline child.

use serde_json::Value;

#[test]
fn captioned_image_hosts_keep_comments_in_their_target() {
    let cases = [
        ("![a](/i) %% c\n^ cap\n", &["image", "comment"][..]),
        ("![a](/i){%%}\n^ cap\n", &["image", "comment"]),
        ("{%%}![a](/i)\n^ cap\n", &["comment", "image"]),
        ("![a][r]{%%}\n^ cap\n\n[r]: /i\n", &["image", "comment"]),
    ];
    for (source, expected) in cases {
        let doc =
            carve::parse_with_options(source, &carve::Options::default().with_positions(true));
        let json = carve::to_json(&doc);
        let tree: Value = serde_json::from_str(&json).unwrap();
        let target = &tree["children"][0]["target"];
        assert_eq!(target["type"], "paragraph", "{source}");
        assert_eq!(target["blockImage"], true, "{source}");
        let kinds: Vec<_> = target["children"]
            .as_array()
            .unwrap()
            .iter()
            .map(|node| node["type"].as_str().unwrap())
            .collect();
        assert_eq!(kinds, expected, "{source}");
        let decoded = carve::from_json(&json).expect("published AST decodes");
        let written = carve::render_carve(&decoded).expect("figure writes");
        let written_tree: Value =
            serde_json::from_str(&carve::to_json(&carve::parse(&written))).unwrap();
        let written_target = &written_tree["children"][0]["target"];
        assert_eq!(written_target["type"], "paragraph", "{written}");
        assert_eq!(written_target["blockImage"], true, "{written}");
        let written_kinds: Vec<_> = written_target["children"]
            .as_array()
            .unwrap()
            .iter()
            .map(|node| node["type"].as_str().unwrap())
            .collect();
        assert_eq!(written_kinds, kinds, "{written}");
    }
}

#[test]
fn sole_image_keeps_the_compact_target() {
    let tree: Value =
        serde_json::from_str(&carve::to_json(&carve::parse("![a](/i)\n^ cap\n"))).unwrap();
    assert_eq!(tree["children"][0]["target"]["type"], "image");
}
