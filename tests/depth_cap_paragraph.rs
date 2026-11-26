//! C2-B-190: a div opener refused at the depth cap is paragraph text.
use carve::api::json;
use serde_json::{Value, json};

#[test]
fn refused_div_lines_continue_one_paragraph() {
    let source = format!(
        "{}::: x\n::: y\n{}",
        "::: note\n".repeat(200),
        ":::\n".repeat(200)
    );
    let reply: Value = serde_json::from_str(&json::call(
        &json!({"call":"render", "source":source}).to_string(),
    ))
    .unwrap();
    let html = reply["ok"]["html"].as_str().unwrap();
    assert_eq!(html.matches("<p>").count(), 1);
    assert!(html.contains("<p>::: x\n::: y</p>"));
}
