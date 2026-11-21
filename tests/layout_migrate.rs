//! C2-A-178: retired spelling survives migration in the sidecar.

use carve::api::{self, MigrateOptions};
use serde_json::{Value, json};

#[test]
fn migrated_spelling_is_beside_the_tree() {
    let old = json!({"type":"document","astVersion":"1.2","children":[{
        "type":"list","ordered":true,"bareMarker":true,"tight":true,"items":[{
            "type":"list_item","children":[{"type":"paragraph","children":[{"type":"text","value":"a"}]}]
        }]
    }]});
    let migrated = api::migrate_tree_with(&old, &MigrateOptions::default()).unwrap();
    assert_eq!(
        migrated.layout,
        json!({
            "version":2,"encoding":"utf-8","source":"","lineEndings":"none","bom":false,
            "nodes":[{"path":"/children/0","startByte":0,"endByte":0,"bare":true}]
        })
    );
    assert!(!api::encode_tree(&migrated.tree).contains("__layout"));
    let response: Value = serde_json::from_str(&api::json::call(
        &json!({"call":"migrateTree","tree":old}).to_string(),
    ))
    .unwrap();
    assert_eq!(response["ok"]["layout"], migrated.layout);
}

#[test]
fn source_supplies_measured_layout_for_migration() {
    let old = json!({"type":"document","astVersion":"2.0","children":[]});
    let source = "# Title\n".to_string();
    let migrated = api::migrate_tree_with(
        &old,
        &MigrateOptions {
            source: Some(source.clone()),
        },
    )
    .unwrap();
    let parsed = api::parse_with_layout(source.as_str(), &carve::Options::default()).unwrap();
    assert_eq!(migrated.layout, parsed.layout);
}
