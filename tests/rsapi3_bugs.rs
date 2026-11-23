//! carve-rs#4350, carve-rs#4351 and the inline source form loss.

use carve::api::json::call;
use serde_json::{Value, json};

fn ok(request: &Value) -> Value {
    let out: Value = serde_json::from_str(&call(&request.to_string())).unwrap();
    out.get("ok").cloned().unwrap_or_else(|| panic!("{out}"))
}

fn codes(result: &Value) -> Vec<String> {
    result["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["code"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn c2_b_046_a_task_box_is_no_blank_reference_label() {
    for source in [
        "- [ ] task\n",
        "> - [ ] task\n",
        "1. [ ] task\n",
        "  - [ ] nested\n",
        // C2-I-072: a bracket pair with nothing glued to it is no reference, so it holds no label.
        "- [ ] task [ ] more\n",
        "[ ] para\n",
        "- [  ] two\n",
    ] {
        let lint = ok(&json!({"call": "lint", "source": source, "options": {"upgrade": true}}));
        assert!(codes(&lint).is_empty(), "{source:?}: {lint}");
    }
    for source in ["- [ ] task [a][ ]\n", "[ ]: u\n", "- [ ] see [^ ]\n"] {
        let lint = ok(&json!({"call": "lint", "source": source, "options": {"upgrade": true}}));
        assert_eq!(codes(&lint), ["blank-reference-label"], "{source:?}");
    }
}

#[test]
fn c2_n_063_include_unresolved_names_the_path() {
    let r = ok(&json!({
        "call": "renderWithDiagnostics",
        "source": "{{ part.crv }}\n\n> {{ q.crv }}\n",
    }));
    let messages: Vec<&str> = r["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["code"] == "include-unresolved")
        .map(|d| d["message"].as_str().unwrap())
        .collect();
    assert_eq!(
        messages,
        [
            "Include \"part.crv\" could not be resolved.",
            "Include \"q.crv\" could not be resolved.",
        ]
    );
}

#[cfg(feature = "fs")]
#[test]
fn c2_n_063_include_unresolved_names_the_path_under_a_root() {
    let root = std::env::temp_dir().join(format!("carve-rsapi3-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let r = ok(&json!({
        "call": "renderWithDiagnostics",
        "source": "{{ missing.crv }}",
        "options": {"includes": {"root": root.to_str().unwrap()}},
    }));
    let unresolved = r["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["code"] == "include-unresolved")
        .unwrap();
    assert_eq!(
        unresolved["message"],
        "Include \"missing.crv\" could not be resolved."
    );
}

#[test]
fn c2_x_023_the_inline_source_form_reports_no_loss() {
    for source in [
        ":wiki[Page] and :cite[doe]{locator=\"p. 4\"}\n",
        ":wiki[Page *x*]\n",
        "*:wiki[P]*\n",
        "- :cite[x]{.c}\n",
    ] {
        let parsed = ok(&json!({
            "call": "parse",
            "source": source,
            "positions": false,
            "options": {"extensions": ["carve.wiki", "carve.cite"]},
        }));
        let written = ok(&json!({"call": "writeWithDiagnostics", "tree": parsed["tree"]}));
        assert_eq!(written["output"], source);
        assert!(codes(&written).is_empty(), "{source:?}: {written}");
    }
}
