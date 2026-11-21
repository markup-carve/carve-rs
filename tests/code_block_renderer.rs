//! The host code-block renderer on render (C2-H-206, C2-H-207,
//! carve-rs#4337), through the library and the JSON boundary.

use carve::api;
use carve::include::{Cause, Includes, Request};
use carve::options::{CodeBlockRenderer, Options};
use serde_json::Value;
use std::sync::Arc;

fn highlight() -> CodeBlockRenderer {
    CodeBlockRenderer::new(|node, default_html| {
        (node["syntax"] == "ts").then(|| format!("<div class=\"hl\">{default_html}</div>"))
    })
}

fn render(source: &str, opts: &Options) -> String {
    api::render(source, opts).unwrap()
}

fn with_renderer() -> Options {
    Options {
        code_block_renderer: Some(highlight()),
        ..Options::default()
    }
}

#[test]
fn a_block_in_a_list_item_is_replaced_at_its_indentation() {
    assert_eq!(
        render("- a\n\n  ```ts\n  x\n  ```\n", &with_renderer()),
        "<ul>\n  <li>a\n    <div class=\"hl\"><pre><code class=\"language-ts\">x\n</code></pre></div>\n  </li>\n</ul>"
    );
}

#[test]
fn a_block_in_a_div_is_replaced() {
    assert_eq!(
        render("::: box\n```ts\nx\n```\n:::\n", &with_renderer()),
        "<div class=\"box\">\n  <div class=\"hl\"><pre><code class=\"language-ts\">x\n</code></pre></div>\n</div>"
    );
}

#[test]
fn a_block_of_an_expanded_include_is_replaced() {
    let resolver = Arc::new(|r: &Request<'_>| {
        (r.path == "a.crv")
            .then(|| (b"```ts\nx\n```\n".to_vec(), "a.crv".to_string()))
            .ok_or(Cause::NotFound)
    });
    let mut includes = Includes::new(resolver);
    includes.file = Some("main.crv".into());
    let opts = Options {
        includes: Some(includes),
        ..with_renderer()
    };
    assert_eq!(
        render("{{ a.crv }}\n", &opts),
        "<div class=\"hl\"><pre><code class=\"language-ts\">x\n</code></pre></div>"
    );
}

#[test]
fn no_result_keeps_the_default_html() {
    let default = render("```py\nx\n```\n", &Options::default());
    assert_eq!(render("```py\nx\n```\n", &with_renderer()), default);
    assert_eq!(default, "<pre><code class=\"language-py\">x\n</code></pre>");
}

#[test]
fn the_function_receives_the_node_and_runs_on_a_supplied_tree() {
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let log = Arc::clone(&seen);
    let opts = Options {
        code_block_renderer: Some(CodeBlockRenderer::new(move |node, _| {
            log.lock().unwrap().push(node.clone());
            None
        })),
        ..Options::default()
    };
    let tree = api::parse("{#c}\n```ts \"T\"\nlet a\n```\n", &Options::default()).unwrap();
    api::render_tree(&tree, &opts).unwrap();
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0]["syntax"], "ts");
    assert_eq!(seen[0]["value"], "let a\n");
    assert_eq!(seen[0]["attrs"]["id"], "c");
}

/// The cases of `clause-cases.json` that name the option, through the
/// boundary's harness form; the include case runs above through a resolver.
#[test]
fn the_json_boundary_takes_the_harness_function_of_the_clause_cases() {
    let text = std::fs::read_to_string("spec-conformance/clause-cases.json").unwrap();
    let mut de = serde_json::Deserializer::from_str(&text);
    de.disable_recursion_limit();
    let all: Value = serde::Deserialize::deserialize(&mut de).unwrap();
    let list = all.get("cases").unwrap_or(&all).as_array().unwrap();
    let mut checked = 0;
    for case in list {
        if case.pointer("/options/codeBlockRenderer").is_none() || case.get("files").is_some() {
            continue;
        }
        let request = serde_json::json!({
            "call": "renderWithDiagnostics",
            "source": case["input"],
            "options": case["options"],
        });
        let response: Value = serde_json::from_str(&api::json::call(&request.to_string())).unwrap();
        if let Some(refusal) = case.pointer("/expected/refusal") {
            assert_eq!(
                response["error"]["name"], refusal["error"],
                "{}",
                case["id"]
            );
            assert!(
                response["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains("codeBlockRenderer")
            );
        } else {
            assert_eq!(
                response["ok"]["html"], case["expected"]["html"],
                "{}",
                case["id"]
            );
        }
        checked += 1;
    }
    assert_eq!(checked, 14);
}
