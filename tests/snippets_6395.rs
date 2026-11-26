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
