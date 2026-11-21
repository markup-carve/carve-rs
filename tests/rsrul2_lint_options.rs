use carve::api::json;
use serde_json::{Value, json};

fn lint(source: &str, options: &Value) -> Value {
    let request = json!({"call":"lint","source":source,"options":options});
    serde_json::from_str(&json::call(&request.to_string())).expect("JSON reply")
}

fn codes(reply: &Value) -> Vec<&str> {
    reply["ok"]["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .filter_map(|entry| entry["code"].as_str())
        .collect()
}

#[test]
fn registered_extension_can_supply_fragment_ids() {
    let input = "[x](#missing)\n";
    assert!(codes(&lint(input, &json!({}))).contains(&"broken-fragment-link"));
    assert!(
        !codes(&lint(input, &json!({"extensions":["carve.tabs"]})))
            .contains(&"broken-fragment-link")
    );
}

#[test]
fn heading_id_policy_reaches_fragment_lint() {
    let input = "= Alpha\n\nSee [a](#alpha).\n";
    assert!(!codes(&lint(input, &json!({"headingIds":"lower"}))).contains(&"broken-fragment-link"));
    assert!(codes(&lint(input, &json!({"headingIds":"off"}))).contains(&"broken-fragment-link"));
}
