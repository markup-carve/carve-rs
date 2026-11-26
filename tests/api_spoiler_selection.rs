use carve::api::json;
use serde_json::{Value, json};

fn parse(extension: &str) -> Value {
    let request = json!({
        "call": "parse",
        "source": ":spoiler[inline]\n\n::: spoiler \"Hint\"\nAnswer.\n:::\n",
        "options": {"extensions": [extension]},
    });
    serde_json::from_str(&json::call(&request.to_string())).unwrap()
}

#[test]
fn each_spoiler_name_claims_only_its_own_form() {
    let block = parse("carve.spoilerBlock");
    assert_eq!(
        block["ok"]["tree"]["children"][0]["children"][0]["name"],
        "spoiler"
    );
    assert_eq!(
        block["ok"]["tree"]["children"][1]["name"],
        "carve.spoilerBlock"
    );

    let inline = parse("carve.spoiler");
    assert_eq!(
        inline["ok"]["tree"]["children"][0]["children"][0]["name"],
        "carve.spoiler"
    );
    assert_eq!(inline["ok"]["tree"]["children"][1]["type"], "div");
}
