//! The engine has two collectors for a container's body, and they must not
//! disagree about the same document.
//!
//! `collect_indented_block_plain_with` runs when neither source lines nor
//! positions are requested - the path `to_html` takes. When either IS requested
//! the mapped twin runs, and that is the path `--json` and `fmt` take.
//! markup-carve/carve-rs#911 fixed the trailing blank inside an open fence in
//! the plain collector only, so one engine parsed
//!
//! ```text
//! - ```
//!   x
//!
//! ```
//!
//! two ways: `to_html` kept the blank, `--json` reported the code block's
//! content without it, and `fmt` wrote a document that re-parsed differently.
//!
//! Asserting the two paths agree is stronger than asserting either one's
//! output, because it fails for ANY future divergence between them, not just
//! for a lost blank line.

use carve::Options;

fn code_content(source: &str, options: &Options<'_>) -> Vec<String> {
    let json: serde_json::Value =
        serde_json::from_str(&carve::to_json_with_options(source, options)).expect("ast json");
    let mut found = Vec::new();
    collect(&json, &mut found);
    found
}

fn collect(node: &serde_json::Value, out: &mut Vec<String>) {
    if let Some(map) = node.as_object() {
        if map.get("type").and_then(|t| t.as_str()) == Some("code_block") {
            if let Some(content) = map.get("content").and_then(|c| c.as_str()) {
                out.push(content.to_string());
            }
        }
        for value in map.values() {
            collect(value, out);
        }
    } else if let Some(items) = node.as_array() {
        for item in items {
            collect(item, out);
        }
    }
}

#[test]
fn the_plain_and_mapped_collectors_agree() {
    for source in [
        "- ```\n  x\n\n",
        "- ```\n  x\n\n\n",
        "1. ```\n   x\n\n",
        "::: note\n```\nx\n\n:::\n",
        "> ```\n> x\n>\n",
        "```\nx\n\n",
        // CONTROLS: these never differed between the paths, and no mutation of
        // the blank-collection rule moves them. They bound the change.
        "- ```\n  x\n\n  ```\n",
        "- ```\n  x\n\n  y\n",
        "```\nx\n",
    ] {
        let plain = code_content(source, &Options::default());
        let mapped = code_content(source, &Options::default().with_positions(true));
        assert_eq!(
            plain, mapped,
            "the two collectors disagree for {source:?}: plain={plain:?} mapped={mapped:?}"
        );
    }
}
