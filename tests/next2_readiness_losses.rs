use serde_json::{Value, json};

fn call(request: &Value) -> Value {
    serde_json::from_str(&carve::api::json::call(&request.to_string())).unwrap()
}

#[test]
fn supplied_tree_losses_name_the_node_and_destination_field() {
    let tree = json!({
        "type":"document", "astVersion":"2.0",
        "extensions":[{"name":"ex.box","version":"1","required":false,"slots":[]}],
        "children":[{"type":"block_extension","name":"ex.box","version":"1",
            "children":[],"fallback":[{"type":"paragraph","children":[
                {"type":"link","href":"javascript:x","children":[{"type":"text","value":"a"}]}
            ]}]}]
    });
    let reply = call(&json!({"call":"renderWithDiagnostics","tree":tree}));
    let diagnostics = reply["ok"]["diagnostics"].as_array().unwrap();
    assert_eq!(diagnostics[0]["path"], "/children/0");
    assert_eq!(
        diagnostics[1]["path"],
        "/children/0/fallback/0/children/0/href"
    );
}

#[test]
fn denied_attribute_does_not_steal_a_later_link_path() {
    let tree = json!({
        "type":"document", "astVersion":"2.0", "children":[
            {"type":"paragraph","attrs":{"keyValues":[["ping","https://x.test"]]},
                "children":[{"type":"link","href":"https://y.test",
                    "children":[{"type":"text","value":"a"}]}]}
        ]
    });
    let reply = call(&json!({"call":"renderWithDiagnostics","tree":tree,
        "options":{"urlPolicy":{"deny":["https"]}}}));
    let destinations: Vec<_> = reply["ok"]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|entry| entry["code"] == "destination-denied")
        .collect();
    assert_eq!(destinations.len(), 2);
    assert_eq!(destinations[0]["path"], "/children/0/attrs/keyValues/0/1");
    assert_eq!(destinations[1]["path"], "/children/0/children/0/href");
}

#[test]
fn denied_link_in_admonition_title_keeps_its_slot_path() {
    let tree = json!({"type":"document","astVersion":"2.0","children":[
        {"type":"admonition","kind":"note","children":[],"title":[
            {"type":"link","href":"javascript:x","children":[{"type":"text","value":"a"}]}
        ]}
    ]});
    let reply = call(&json!({"call":"renderWithDiagnostics","tree":tree}));
    assert_eq!(
        reply["ok"]["diagnostics"][0]["code"], "destination-denied",
        "{reply}"
    );
    assert_eq!(
        reply["ok"]["diagnostics"][0]["path"],
        "/children/0/title/0/href"
    );
}

#[test]
fn denied_link_in_caption_keeps_its_singular_slot_path() {
    let tree = json!({"type":"document","astVersion":"2.0","children":[
        {"type":"paragraph","role":"image","children":[{"type":"image","src":"a.png","alt":"a"}],
            "caption":{"type":"caption","children":[{"type":"paragraph","children":[
                {"type":"link","href":"javascript:x","children":[{"type":"text","value":"b"}]}
            ]}]}}
    ]});
    let reply = call(&json!({"call":"renderWithDiagnostics","tree":tree}));
    assert_eq!(
        reply["ok"]["diagnostics"][0]["code"], "destination-denied",
        "{reply}"
    );
    assert_eq!(
        reply["ok"]["diagnostics"][0]["path"],
        "/children/0/caption/children/0/children/0/href"
    );
}

#[test]
fn tabs_wrapper_keeps_authored_attributes_in_renderer_order() {
    let reply = call(&json!({
        "call":"render",
        "source":"{#t .k key=v onclick=evil data-src=javascript:evil}\n:::: tabs\n::: tab \"A\"\na\n:::\n::::\n",
        "options":{"extensions":["carve.tabs"],"renderers":{"carve.tabs":"standard"}}
    }));
    let html = reply["ok"]["html"].as_str().unwrap();
    assert!(
        html.starts_with(
            "<div id=\"t\" class=\"tabs k\" key=\"v\" data-src=\"\" role=\"group\" aria-label=\"Tabs\">"
        ),
        "{html}"
    );
    assert!(!html.contains("onclick="), "{html}");
    assert!(html.contains("data-src=\"\""), "{html}");
}

#[test]
fn tabs_wrapper_replaces_generated_role_without_case_sensitivity() {
    let reply = call(&json!({
        "call":"render",
        "source":"{#t .k ROLE=note}\n:::: tabs\n::: tab \"A\"\na\n:::\n::::\n",
        "options":{"extensions":["carve.tabs"],"renderers":{"carve.tabs":"standard"}}
    }));
    let html = reply["ok"]["html"].as_str().unwrap();
    assert!(
        html.starts_with("<div id=\"t\" class=\"tabs k\" ROLE=\"note\" aria-label=\"Tabs\">"),
        "{html}"
    );
}

#[test]
fn writer_loss_messages_use_the_catalog_template() {
    let tree = json!({"type":"document","astVersion":"2.0","children":[
        {"type":"paragraph","role":"image","children":[{"type":"image","src":"i.png","decorative":true}]}
    ]});
    let reply = call(&json!({"call":"writeWithDiagnostics","tree":tree}));
    let diagnostic = reply["ok"]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["path"] == "/children/0/children/0/decorative")
        .expect("decorative field diagnostic");
    assert_eq!(diagnostic["code"], "field-unspellable");
    assert_eq!(diagnostic["path"], "/children/0/children/0/decorative");
    assert!(
        diagnostic["message"]
            .as_str()
            .unwrap()
            .starts_with("Carve source cannot spell ")
    );
}

#[test]
fn source_only_write_error_carries_loss_diagnostics() {
    let tree = json!({"type":"document","astVersion":"2.0","children":[
        {"type":"paragraph","role":"image","children":[
            {"type":"image","src":"i.png","decorative":true}
        ]}
    ]});
    let reply = call(&json!({"call":"write","tree":tree}));
    assert_eq!(reply["error"]["name"], "CarveWriteError");
    let diagnostics = reply["error"]["diagnostics"].as_array().unwrap();
    assert_eq!(diagnostics[0]["code"], "field-unspellable");
    assert_eq!(diagnostics[0]["path"], "/children/0/children/0/decorative");
}

#[test]
fn renderer_syntax_filter_does_not_match_a_block_without_syntax() {
    let reply = call(&json!({
        "call":"render", "source":"```\nx\n```\n",
        "options":{"codeBlockRenderer":{"syntax":"","result":"<b>matched</b>"}}
    }));
    assert_eq!(reply["ok"]["html"], "<pre><code>x\n</code></pre>");
}
