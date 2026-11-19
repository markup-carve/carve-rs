//! `allowLosses` and `maxLosses` of the importer (CC-1260, CC-1261) and the
//! `path` of an error on the JSON boundary (CC-507, C2-A-167).

use carve::api::{self, ImportOptions, LossOptions, json::call};
use serde_json::{Value, json};

fn reply(request: &Value) -> Value {
    serde_json::from_str(&call(&request.to_string())).unwrap()
}

fn corpus() -> Value {
    let path = format!(
        "{}/spec-conformance/clause-cases.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(path).unwrap();
    let mut de = serde_json::Deserializer::from_str(&text);
    de.disable_recursion_limit();
    serde::Deserialize::deserialize(&mut de).unwrap()
}

#[test]
fn cc_1260_an_allowed_code_fails_no_strict_import() {
    let r = reply(&json!({
        "call": "importDocument", "from": "html", "source": "<p>a<wbr>b</p>",
        "options": {"allowLosses": ["import-element-dropped"], "strictLosses": true},
    }));
    assert_eq!(r["ok"]["source"], "ab\n");
    assert_eq!(r["ok"]["diagnostics"], json!([]));
    let strict = reply(&json!({
        "call": "importDocument", "from": "html", "source": "<p>a<wbr>b</p>",
        "options": {"strictLosses": true},
    }));
    assert_eq!(strict["error"]["name"], "CarveRenderError");
}

#[test]
fn cc_1261_max_losses_caps_the_import_record() {
    let r = reply(&json!({
        "call": "importDocument", "from": "html", "source": "<p>a<wbr>b<wbr>c</p>",
        "options": {"maxLosses": 1},
    }));
    let codes: Vec<&str> = r["ok"]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["code"].as_str().unwrap())
        .collect();
    assert_eq!(codes, ["import-element-dropped"]);
    assert_eq!(r["ok"]["suppressedDiagnostics"], 1);
}

#[test]
fn loss_options_in_rust() {
    let limits = LossOptions {
        max_losses: 0,
        ..LossOptions::default()
    };
    let imported = api::Carve::default()
        .import_document_with("<p>a<wbr>b</p>", &ImportOptions::default(), &limits)
        .unwrap();
    assert!(imported.diagnostics.is_empty());
    assert_eq!(imported.suppressed_diagnostics, 1);
    assert_eq!(LossOptions::default().max_losses, 100);
}

#[test]
fn cc_507_an_option_error_names_its_path() {
    let r = reply(&json!({"call": "format", "source": "a\n", "options": {"profile": "comment"}}));
    assert_eq!(r["error"]["name"], "CarveOptionError");
    assert_eq!(r["error"]["path"], "/profile");
}

/// C2-A-167: an error that names the node at `path` or a field below it agrees.
#[test]
fn c2_a_167_refusal_paths_of_the_clause_cases() {
    let calls = [
        ("parse", "parseWithDiagnostics"),
        ("render", "renderWithDiagnostics"),
        ("format", "formatWithDiagnostics"),
        ("tree", "validateTree"),
    ];
    let mut checked = 0;
    for c in corpus()["cases"].as_array().unwrap() {
        let Some(want) = c["expected"].get("refusal") else {
            continue;
        };
        let kind = c["kind"].as_str().unwrap_or("render");
        let Some((_, name)) = calls.iter().find(|(k, _)| *k == kind) else {
            continue;
        };
        if c.get("files").is_some() || c.get("host").is_some() || c.get("json").is_some() {
            continue;
        }
        let mut request = json!({"call": name, "options": c["options"]});
        match c.get("tree") {
            Some(tree) => request["tree"] = tree.clone(),
            None => request["source"] = c["input"].clone(),
        }
        let error = reply(&request)["error"].clone();
        assert_eq!(error["name"], want["error"], "{}: {error}", c["id"]);
        if let Some(path) = want["path"].as_str() {
            let got = error["path"].as_str().unwrap_or("<none>");
            assert!(
                got == path || got.starts_with(&format!("{path}/")),
                "{}: want {path}, got {got}",
                c["id"]
            );
        }
        checked += 1;
    }
    assert!(checked >= 20, "{checked}");
}
