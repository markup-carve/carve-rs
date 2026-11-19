//! PROVISIONAL standard extension `carve.tabs` through the JSON boundary
//! (C2-X-039 to C2-X-048, C2-X-056), driven by the clause cases that enable it.
mod common;

use carve2::api;
use common::{parse_json, sorted};
use serde_json::{Value, json};

fn call(request: &Value) -> Value {
    parse_json(&api::json::call(&request.to_string()))
}

fn without_pos(v: &mut Value) {
    match v {
        Value::Object(m) => {
            m.remove("pos");
            m.values_mut().for_each(without_pos);
        }
        Value::Array(a) => a.iter_mut().for_each(without_pos),
        _ => {}
    }
}

/// Checks one case and returns a failure line, if any.
fn check(case: &Value) -> Option<String> {
    let id = case["id"].as_str().unwrap_or("");
    let input = case["input"].as_str().unwrap_or("");
    let expected = &case["expected"];
    let mut failures = Vec::new();
    if let Some(html) = expected["html"].as_str() {
        let out = call(&json!({"call": "renderWithDiagnostics", "source": input, "options": case["options"]}));
        let got = out["ok"]["html"].as_str().unwrap_or("");
        if got != html {
            failures.push(format!("html\n  want {html}\n  got  {got} {}", out["error"]));
        }
        for code in expected["notReported"].as_array().into_iter().flatten() {
            let codes = out["ok"]["diagnostics"].as_array().cloned().unwrap_or_default();
            if codes.iter().any(|d| d["code"] == *code) {
                failures.push(format!("reported {code}"));
            }
        }
    }
    if let Some(children) = expected.get("children") {
        let mut options = case["options"].clone();
        options.as_object_mut().map(|o| o.remove("renderers"));
        let out = call(&json!({"call": "parse", "source": input, "options": options}));
        let mut got = out["ok"]["tree"]["children"].clone();
        without_pos(&mut got);
        let (want, got) = (sorted(children), sorted(&got));
        if want != got {
            failures.push(format!("children\n  want {want}\n  got  {got} {}", out["error"]));
        }
    }
    (!failures.is_empty()).then(|| format!("{id}: {}", failures.join("; ")))
}

fn pinned_cases() -> Vec<Value> {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/spec-conformance/clause-cases.json"
    ))
    .unwrap_or_default();
    parse_json(&text)["cases"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| matches!(c["kind"].as_str(), Some("parse" | "render")))
        .filter(|c| c["options"].get("extensions").is_some())
        .cloned()
        .collect()
}

#[test]
fn c2_x_039_the_pinned_tabs_cases() {
    let cases = pinned_cases();
    assert!(cases.len() >= 20, "{}", cases.len());
    let failures: Vec<String> = cases.iter().filter_map(check).collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Ruling D6 (carve#6070): clause cases of the spec after the engine pin.
#[test]
fn c2_x_056_selected_tab_and_static_sections() {
    let tabs = |input: &str, mode: Option<&str>, renderer: bool, html: &str| {
        let mut options = json!({"extensions": ["carve.tabs"]});
        if renderer {
            options["renderers"] = json!({"carve.tabs": "standard"});
        }
        if let Some(mode) = mode {
            options["mode"] = mode.into();
        }
        json!({"id": input, "input": input, "options": options, "expected": {"html": html}})
    };
    let two = ":::: tabs\n::: tab \"Install\"\nRun setup.\n:::\n\n{selected}\n::: tab \"Use\"\nOpen the app.\n:::\n::::\n";
    let cases = [
        tabs(":::: tabs\n::: tab \"Install\"\nRun.\n:::\n::::\n", Some("static"), true,
            "<div class=\"tabs\" role=\"group\" aria-label=\"Tabs\"><section class=\"tabs-panel\"><h3 class=\"tabs-label\">Install</h3><p>Run.</p></section></div>"),
        tabs(":::: tabs\n::: tab \"Install\"\nRun.\n:::\n::::\n", Some("static"), false,
            "<p><strong>Install</strong></p>\n<p>Run.</p>"),
        tabs(":::: tabs\n::: tab \"A\"\na\n:::\n{selected}\n::: tab \"B\"\nb\n:::\n::::\n", None, true,
            "<div class=\"tabs\" role=\"group\" aria-label=\"Tabs\"><input type=\"radio\" name=\"tabs-1\" id=\"tabs-1-0\"><label for=\"tabs-1-0\">A</label><input type=\"radio\" name=\"tabs-1\" id=\"tabs-1-1\" checked><label for=\"tabs-1-1\">B</label><div class=\"tab-panel\" role=\"group\" aria-label=\"A\"><p>a</p></div><div class=\"tab-panel\" role=\"group\" aria-label=\"B\"><p>b</p></div></div>"),
        tabs(":::: tabs\n::: tab \"A\"\na\n:::\n\n{selected}\n::: tab \"B\"\nb\n:::\n\n{selected}\n::: tab \"C\"\nc\n:::\n::::\n", None, true,
            "<div class=\"tabs\" role=\"group\" aria-label=\"Tabs\"><input type=\"radio\" name=\"tabs-1\" id=\"tabs-1-0\"><label for=\"tabs-1-0\">A</label><input type=\"radio\" name=\"tabs-1\" id=\"tabs-1-1\" checked><label for=\"tabs-1-1\">B</label><input type=\"radio\" name=\"tabs-1\" id=\"tabs-1-2\"><label for=\"tabs-1-2\">C</label><div class=\"tab-panel\" role=\"group\" aria-label=\"A\"><p>a</p></div><div class=\"tab-panel\" role=\"group\" aria-label=\"B\"><p>b</p></div><div class=\"tab-panel\" role=\"group\" aria-label=\"C\"><p>c</p></div></div>"),
        tabs(":::: tabs\n::: tab \"A\"\na\n:::\n\n{k=v}\n::: tab \"B\"\nb\n:::\n::::\n", None, true,
            "<div class=\"tabs\" role=\"group\" aria-label=\"Tabs\"><input type=\"radio\" name=\"tabs-1\" id=\"tabs-1-0\" checked><label for=\"tabs-1-0\">A</label><input type=\"radio\" name=\"tabs-1\" id=\"tabs-1-1\"><label for=\"tabs-1-1\">B</label><div class=\"tab-panel\" role=\"group\" aria-label=\"A\"><p>a</p></div><div class=\"tab-panel\" role=\"group\" aria-label=\"B\"><p>b</p></div></div>"),
        tabs(two, Some("static"), true,
            "<div class=\"tabs\" role=\"group\" aria-label=\"Tabs\"><section class=\"tabs-panel\"><h3 class=\"tabs-label\">Install</h3><p>Run setup.</p></section><section class=\"tabs-panel\"><h3 class=\"tabs-label\">Use</h3><p>Open the app.</p></section></div>"),
    ];
    let failures: Vec<String> = cases.iter().filter_map(check).collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn tabs_json_option_errors() {
    for options in [
        json!({"extensions": ["carve.nope"]}),
        json!({"extensions": "carve.tabs"}),
        json!({"extensions": ["carve.tabs"], "renderers": {"carve.tabs": "mine"}}),
        json!({"renderers": {"carve.tabs": "standard"}}),
    ] {
        let out = call(&json!({"call": "render", "source": "x", "options": options}));
        assert_eq!(out["error"]["name"], "CarveOptionError", "{options}");
    }
}

#[test]
fn the_tabs_renderer_reads_a_supplied_tree() {
    let tab = |title: &str, body: &str| {
        json!({"type": "block_extension", "name": "carve.tab", "version": "1", "children": [],
            "slots": {"title": [{"type": "text", "value": title}],
                "body": [{"type": "paragraph", "children": [{"type": "text", "value": body}]}]},
            "fallback": [{"type": "paragraph", "children": [{"type": "text", "value": body}]}]})
    };
    let tree = json!({"type": "document", "astVersion": "2.0",
        "extensions": [
            {"name": "carve.tab", "version": "1", "required": false, "slots": ["title", "body"]},
            {"name": "carve.tabs", "version": "1", "required": false, "slots": ["tabs"]}],
        "children": [{"type": "block_extension", "name": "carve.tabs", "version": "1", "children": [],
            "slots": {"tabs": [tab("A & B", "x")]},
            "fallback": [{"type": "paragraph", "children": [{"type": "text", "value": "x"}]}]}]});
    let out = call(&json!({"call": "renderWithDiagnostics", "tree": tree,
        "options": {"extensions": ["carve.tabs"], "renderers": {"carve.tabs": "standard"}}}));
    assert_eq!(
        out["ok"]["html"],
        "<div class=\"tabs\" role=\"group\" aria-label=\"Tabs\"><input type=\"radio\" name=\"tabs-1\" id=\"tabs-1-0\" checked><label for=\"tabs-1-0\">A &amp; B</label><div class=\"tab-panel\" role=\"group\" aria-label=\"A &amp; B\"><p>x</p></div></div>"
    );
    assert_eq!(out["ok"]["diagnostics"], json!([]));
}
