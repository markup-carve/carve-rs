//! A migrated image paragraph can have invisible children before its image (C2-A-136, C2-A-184).

use carve::api::{self, MigrateOptions};
use serde_json::{Value, json};

#[test]
fn old_image_promotion_with_a_leading_comment_migrates() {
    let old = json!({"type":"document","children":[{"type":"paragraph","blockImage":true,
        "children":[{"type":"comment","block":false,"delimited":true,"content":""},
                    {"type":"image","src":"x.png","alt":"alt"}]}]});
    let migrated = api::migrate_tree_with(&old, &MigrateOptions::default()).unwrap();
    let got: Value = serde_json::from_str(&api::encode_tree(&migrated.tree)).unwrap();
    assert_eq!(got["children"][0]["role"], "image");
    assert_eq!(got["children"][0]["children"], json!([
        {"type":"comment","form":"braced","value":""},
        {"type":"image","src":"x.png","alt":"alt"}
    ]));
    assert!(migrated.diagnostics.is_empty());
}

#[test]
fn role_shape_accepts_invisible_children_on_either_side() {
    for i in 0..1000 {
        let kind = if i % 2 == 0 { "image" } else { "math" };
        let role_node = if kind == "image" {
            json!({"type":"image","src":"x.png","alt":"alt"})
        } else {
            json!({"type":"math","display":true,"value":"x"})
        };
        let mut children = Vec::new();
        for _ in 0..i % 4 {
            children.push(json!({"type":"comment","form":"braced","value":""}));
        }
        children.push(role_node);
        for _ in 0..i / 4 % 4 {
            children.push(json!({"type":"soft_break"}));
        }
        let tree = json!({"type":"document","astVersion":"2.0","children":[
            {"type":"paragraph","role":kind,"children":children}
        ]});
        api::decode_tree(&tree.to_string()).unwrap_or_else(|e| panic!("variant {i}: {e}"));
    }
}

#[test]
fn role_shape_rejects_multiple_images_and_unrelated_text() {
    for children in [
        json!([{"type":"image","src":"x"},{"type":"image","src":"y"}]),
        json!([{"type":"comment","form":"braced","value":""},
               {"type":"text","value":"x"},{"type":"image","src":"x"}]),
    ] {
        let tree = json!({"type":"document","astVersion":"2.0","children":[
            {"type":"paragraph","role":"image","children":children}
        ]});
        assert!(api::decode_tree(&tree.to_string()).is_err());
    }
}
