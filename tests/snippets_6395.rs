//! Named snippets (carve#6395) in shapes no clause case pins: definitions an
//! include brings, under `roundtrip` and across files.
mod common;

use carve::api;
use common::parse_json;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::Arc;

fn render(source: &str, files: &[(&str, &str)], roundtrip: bool) -> Value {
    let map: BTreeMap<String, Vec<u8>> = files
        .iter()
        .map(|(name, text)| ((*name).to_string(), text.as_bytes().to_vec()))
        .collect();
    let resolver: carve::include::Resolver = Arc::new(move |r: &carve::include::Request<'_>| {
        map.get(r.path)
            .map(|bytes| (bytes.clone(), r.path.to_string()))
            .ok_or(carve::include::Cause::NotFound)
    });
    let request = json!({
        "call": "renderWithDiagnostics",
        "source": source,
        "options": {"roundtrip": roundtrip},
    });
    let out = api::json::call_with_includes(
        &request.to_string(),
        carve::include::Includes::new(resolver),
    );
    parse_json(&out)["ok"].clone()
}

fn codes(out: &Value) -> Vec<(String, Option<String>)> {
    out["diagnostics"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|d| {
            (
                d["code"].as_str().unwrap_or("").to_string(),
                d["file"].as_str().map(str::to_string),
            )
        })
        .collect()
}

/// C2-H-215, C2-H-216: the copy of an included definition resolves in its
/// file, and its carriers and the template name the include scope.
#[test]
fn an_included_definition_carries_its_scope() {
    let out = render(
        "{{ d.crv }}\n\nA {{ $v }}\n",
        &[("d.crv", "{$v}: [x][r]\n\n[r]: /child\n")],
        true,
    );
    assert_eq!(
        out["html"],
        "<template data-carve-snippets data-carve-snippet-scope=\"include\">{$v}: [x][r]</template>\n\
         <p>A <a href=\"/child\" data-carve-ref=\"r\" data-carve-snippet=\"1:inline:$v\" \
         data-carve-snippet-scope=\"include\">x</a></p>"
    );
}

/// C2-N-090: a definition of an included file that loses to the top level is
/// reported once, in its file.
#[test]
fn a_definition_that_loses_across_files_is_reported() {
    let out = render(
        "{$v}: X\n\n{{ d.crv }}\n\n{{ $v }}\n",
        &[("d.crv", "{$v}: Y\n")],
        false,
    );
    assert_eq!(out["html"], "<p>X</p>");
    let found = codes(&out);
    assert_eq!(
        found
            .iter()
            .filter(|(code, _)| code == "duplicate-snippet")
            .collect::<Vec<_>>(),
        [&("duplicate-snippet".to_string(), Some("d.crv".to_string()))]
    );
    let range = out["diagnostics"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|d| d["code"] == "duplicate-snippet")
        .map(|d| d["range"].clone());
    assert_eq!(range, Some(json!({"startOffset": 1, "endOffset": 3})));
}

/// A source without a snippet use renders as before: no carriers, no template.
#[test]
fn a_source_without_uses_takes_no_snippet_path() {
    let out = render("{$v}: X\n\nA b\n", &[], true);
    assert_eq!(
        out["html"],
        "<template data-carve-snippets>{$v}: X</template>\n<p>A b</p>"
    );
}

fn render_transformed(source: &str) -> Value {
    let request = json!({
        "call": "renderWithDiagnostics",
        "source": source,
        "options": {"roundtrip": true},
        "host": {"documentTransforms": [{
            "name": "sample.a",
            "after": "/children/2",
            "insert": {"type": "paragraph", "children": [{"type": "text", "value": "b"}]},
        }]},
    });
    parse_json(&api::json::call(&request.to_string()))["ok"].clone()
}

/// C2-H-215: the nodes a document transform keeps carry their snippet carriers
/// and note marks into the render of the view it returns.
#[test]
fn carriers_survive_a_transform_that_keeps_their_nodes() {
    let out = render_transformed("{$v}: X\n\nA {{ $v }} and {{ $w }}\n\n{$w}: *W*\n");
    assert_eq!(
        out["html"],
        "<template data-carve-snippets>{$v}: X</template>\n\
         <p>A <span data-carve-snippet=\"1:inline:$v\">X</span> and \
         <strong data-carve-snippet=\"2:inline:$w\">W</strong></p>\n\
         <template data-carve-snippets>{$w}: *W*</template>\n<p>b</p>"
    );
    let out = render_transformed("{$s}: x[^n]\n\n[Go {{ $s }}](/u)\n\n[^n]: N\n");
    let html = out["html"].as_str().unwrap_or("");
    assert!(
        html.starts_with(
            "<template data-carve-snippets>{$s}: x[^n]</template>\n\
             <p><a href=\"/u\">Go <span data-carve-snippet=\"1:inline:$s\">x</span></a>\
             <a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\" data-carve-footnote-label=\"n\" \
             data-carve-snippet-mark=\"1\"><sup>1</sup></a></p>\n<p>b</p>\n"
        ),
        "{html}"
    );
}

/// C2-N-090: a file included twice repeats its definitions; the second include's lose.
#[test]
fn a_file_included_twice_reports_its_second_definitions() {
    let out = render(
        "{{ d.crv }}\n\n{{ d.crv }}\n\n{{ $v }}\n",
        &[("d.crv", "{$v}: Y\n")],
        false,
    );
    assert_eq!(out["html"], "<p>Y</p>");
    let found = codes(&out);
    assert_eq!(
        found
            .iter()
            .filter(|(code, _)| code == "duplicate-snippet")
            .count(),
        1,
        "{found:?}"
    );
}

/// C2-H-215: a transform that changes one copy keeps the carriers of that copy
/// and of every later one.
#[test]
fn a_changed_copy_keeps_its_carriers() {
    let request = json!({
        "call": "renderWithDiagnostics",
        "source": "{$v}: X\n\nA {{ $v }} {{ $v }}\n",
        "options": {"roundtrip": true},
        "host": {"documentTransforms": [{
            "name": "sample.b",
            "after": "/children/1/children/1/children/0",
            "setValue": "Z",
        }]},
    });
    let out = parse_json(&api::json::call(&request.to_string()))["ok"].clone();
    assert_eq!(
        out["html"],
        "<template data-carve-snippets>{$v}: X</template>\n\
         <p>A <span data-carve-snippet=\"1:inline:$v\">Z</span> \
         <span data-carve-snippet=\"2:inline:$v\">X</span></p>"
    );
}

/// C2-N-095: a definition of a supplied tree without positions that a transform
/// keeps is no node the transform inserted.
#[test]
fn a_kept_definition_without_a_position_is_no_finding() {
    let request = json!({
        "call": "renderWithDiagnostics",
        "tree": {"type": "document", "astVersion": "2.0", "children": [
            {"type": "snippet_definition", "name": "v", "children": [{"type": "text", "value": "X"}]},
            {"type": "paragraph", "children": [{"type": "snippet_use", "name": "v", "mode": "inline"}]},
        ]},
        "options": {},
        "host": {"documentTransforms": [{
            "name": "sample.a",
            "after": "/children/1",
            "insert": {"type": "paragraph", "children": [{"type": "text", "value": "b"}]},
        }]},
    });
    let out = parse_json(&api::json::call(&request.to_string()))["ok"].clone();
    assert_eq!(out["html"], "<p>X</p>\n<p>b</p>");
    assert_eq!(codes(&out), Vec::<(String, Option<String>)>::new());
}

/// C2-N-091: a host with a registered extension and no transform renders on
/// the arena path, which expands the uses too.
#[test]
fn a_host_with_an_extension_expands_uses() {
    let mut host = api::Carve::default();
    host.register_extension(api::Extension {
        declaration: api::ExtensionDeclaration {
            name: "sample.x".into(),
            version: "1".into(),
            required: false,
            slots: Vec::new(),
        },
        renderers: Vec::new(),
        transform: None,
    })
    .expect("registered");
    let opts = carve::options::Options::default();
    let out = host
        .render_with_diagnostics("{$v}: X\n\nA {{ $v }} {{ $w }}\n", &opts)
        .expect("rendered");
    assert_eq!(out.html, "<p>A X {{ $w }}</p>");
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.code == "snippet-unresolved"),
        "{:?}",
        out.diagnostics
    );
}

/// C2-H-215: a node a transform inserts at a copy's place does not take its carriers.
#[test]
fn an_inserted_sibling_takes_no_carrier() {
    let request = json!({
        "call": "renderWithDiagnostics",
        "source": "{$v}: X\n\n{{ $v }}\n",
        "options": {"roundtrip": true},
        "host": {"documentTransforms": [{
            "name": "sample.a",
            "after": "/children/0",
            "insert": {"type": "paragraph", "children": [{"type": "text", "value": "b"}]},
        }]},
    });
    let out = parse_json(&api::json::call(&request.to_string()))["ok"].clone();
    assert_eq!(
        out["html"],
        "<template data-carve-snippets>{$v}: X</template>\n<p>b</p>\n\
         <p data-carve-snippet=\"1:block:$v\">X</p>"
    );
}

fn roundtrip_html(source: &str) -> String {
    let request = json!({"call": "render", "source": source, "options": {"roundtrip": true}});
    let out = parse_json(&api::json::call(&request.to_string()));
    out["ok"]["html"].as_str().unwrap_or("").to_string()
}

/// C2-H-215: a copy that renders as one element carries the carriers on it: an
/// autolink's `<a>`, and a link whose use inside expands to no note.
#[test]
fn one_element_copies_carry_their_carriers() {
    let html = roundtrip_html("{$v}: <https://example.com>\n\nA {{ $v }}\n");
    assert!(
        html.ends_with(
            "<p>A <a href=\"https://example.com\" data-carve-autolink \
             data-carve-snippet=\"1:inline:$v\">https://example.com</a></p>"
        ),
        "{html}"
    );
    let html = roundtrip_html("{$b}: X\n\n{$a}: [{{ $b }}](/url)\n\nA {{ $a }}\n");
    assert!(
        html.ends_with(
            "<p>A <a href=\"/url\" data-carve-snippet=\"1:inline:$a\">\
             <span data-carve-snippet=\"2:inline:$b\">X</span></a></p>"
        ),
        "{html}"
    );
    // A note mark of the inner use moves out behind the link: the copy is no longer one element.
    let html = roundtrip_html("{$n}: y[^q]\n\n{$a}: [{{ $n }}](/url)\n\nA {{ $a }}\n\n[^q]: Q\n");
    assert!(
        html.contains("<p>A <span data-carve-snippet=\"1:inline:$a\"><a href=\"/url\">"),
        "{html}"
    );
}

/// C2-A-221: a use in a metadata title, an inline list without a node type of its own, is read.
#[test]
fn a_use_in_a_metadata_title_is_read() {
    let tree = json!({"type": "document", "astVersion": "2.0", "children": [
        {"type": "paragraph", "metadata": {"title": [
            {"type": "snippet_use", "name": "v", "mode": "inline"},
        ]}, "children": [{"type": "text", "value": "a"}]},
    ]});
    let out = parse_json(&api::json::call(
        &json!({"call": "validateTree", "tree": tree}).to_string(),
    ));
    assert!(out.get("ok").is_some(), "{out}");
}
