use carve::api::{self, Options};

#[test]
fn role_paragraph_roundtrips_with_comment_lines() {
    for source in ["![a %% b](x)\n%%cap", "![a](x)\n%% comment\n%% next"] {
        let tree = api::parse(source, &Options::default()).unwrap();
        let json = api::encode_tree(&tree);
        let decoded = api::decode_tree(&json).unwrap();
        assert_eq!(api::encode_tree(&decoded), json);
    }
    for index in 0..1_000 {
        let source = format!("![alt{index}](asset{index}.png)\n%% comment{index}");
        let tree = api::parse(source.as_str(), &Options::default()).unwrap();
        let json = api::encode_tree(&tree);
        let decoded = api::decode_tree(&json).unwrap();
        assert_eq!(api::encode_tree(&decoded), json, "variant {index}");
    }
}
