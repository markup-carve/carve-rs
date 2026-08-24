use carve::{html_to_ast, html_to_carve, HtmlImportOptions};

/// The fields PART 12 fills in from a SOURCE - which spelling a marker used,
/// which slot an attribute sat in. An import read HTML and had no source to
/// read one off, so the published tree records none of them
/// (markup-carve/carve#1647), and a comparison against a parse has to look past
/// them for that reason rather than as a convenience. The spec's own reading of
/// these two exits skips the same set
/// (`tests/the-two-import-exits-agree.test.mjs`).
///
/// Plus the two that record WHERE a node was written: a parse read bytes and an
/// import did not, so `pos` and `srcByteLength` differ by construction.
const IGNORED: &[&str] = &[
    "order",
    "bulletChar",
    "bareMarker",
    "delim",
    "definitionLines",
    "definitionSpans",
    "termSpans",
    "pos",
    "srcByteLength",
];

fn comparable(v: &serde_json::Value) -> serde_json::Value {
    match v {
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(comparable).collect())
        }
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.iter()
                .filter(|(key, _)| !IGNORED.contains(&key.as_str()))
                .map(|(key, inner)| (key.clone(), comparable(inner)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn source_layout_keys(v: &serde_json::Value, path: &str, out: &mut Vec<String>) {
    match v {
        serde_json::Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                source_layout_keys(item, &format!("{path}[{index}]"), out);
            }
        }
        serde_json::Value::Object(map) => {
            for (key, inner) in map {
                // `pos` and `srcByteLength` are the tail of `IGNORED` and are
                // location, not layout - the scan is about the seven the spec
                // names.
                if IGNORED[..7].contains(&key.as_str()) {
                    out.push(format!("{path}.{key}"));
                } else {
                    source_layout_keys(inner, &format!("{path}.{key}"), out);
                }
            }
        }
        _ => {}
    }
}

const HTML: &str = r##"<h1 id="Target">Target</h1><p>See <a href="#Target">Target</a>.</p>"##;

/// THE CONTROL FOR carve-rs#1324, and it is the half a fix for the slot can
/// silently undo. `Target` IS the slug `# Target` generates, so a writer told
/// nothing about the id reads it as generated and omits it - which is the loss
/// that ticket closed. Asserting the written source rather than the slot keeps
/// the requirement stated in terms of what a reader loses.
#[test]
fn the_writer_writes_an_authored_id_that_equals_the_generated_slug() {
    let source = html_to_carve(HTML, &HtmlImportOptions::default())
        .unwrap()
        .value;
    assert_eq!(source, "{#Target}\n# Target\n\nSee [Target](#Target).\n");
}

#[test]
fn the_published_tree_keeps_the_authored_id() {
    let imported = html_to_ast(HTML, &HtmlImportOptions::default())
        .unwrap()
        .value;
    let json: serde_json::Value =
        serde_json::from_str(&carve::ast_json::to_json(&imported)).unwrap();
    assert_eq!(json["children"][0]["attrs"]["id"], "Target");
}

/// AND RECORDS NO SPELLING FOR IT (markup-carve/carve#1647). carve-rs#1324
/// carried the id by pushing `AttrSlot::Id` into `attrs.order`, and `order` is a
/// source-layout field: an import read HTML and saw no source, so stating one
/// states a spelling that was never read. The slot is a writer-only channel now
/// - `html_to_carve` above still gets it, `html_to_ast` never does.
#[test]
fn the_published_tree_records_no_source_layout_field() {
    let imported = html_to_ast(HTML, &HtmlImportOptions::default())
        .unwrap()
        .value;
    let json: serde_json::Value =
        serde_json::from_str(&carve::ast_json::to_json(&imported)).unwrap();
    let mut found = Vec::new();
    source_layout_keys(&json, "", &mut found);
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn an_html_heading_id_equal_to_its_slug_stays_authored() {
    let options = HtmlImportOptions::default();
    let source = html_to_carve(HTML, &options).unwrap().value;
    let imported = html_to_ast(HTML, &options).unwrap().value;
    let parsed = carve::parse(&source);

    let parsed_json: serde_json::Value =
        serde_json::from_str(&carve::ast_json::to_json(&parsed)).unwrap();
    let imported_json: serde_json::Value =
        serde_json::from_str(&carve::ast_json::to_json(&imported)).unwrap();
    assert_eq!(comparable(&parsed_json), comparable(&imported_json));
    assert_eq!(parsed.footnote_defs, imported.footnote_defs);
}
