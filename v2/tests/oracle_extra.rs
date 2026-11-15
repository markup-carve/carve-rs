//! Extra inputs for rules no specification example covers, with the oracle's
//! tree and HTML (tests/data/extra.json). The engine renders the oracle's tree
//! and resolves it again after stripping the result fields. Where the oracle
//! contradicts a clause the case carries `expected`, the clause's output
//! (ORACLE-html.md).
mod common;

use carve2::ast::Node;
use carve2::options::{HeadingIds, Mode, Options, Profile, UrlPolicy};
use serde_json::Value;

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

fn map(v: &Value) -> std::collections::BTreeMap<String, String> {
    v.as_object()
        .into_iter()
        .flatten()
        .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
        .collect()
}

fn options_of(opts: &Value) -> Options {
    let mut o = Options::default();
    if let Some(h) = opts["headingIds"].as_str().and_then(HeadingIds::from_name) {
        o.heading_ids = h;
    }
    if let Some(p) = opts["profile"].as_str().and_then(Profile::from_name) {
        o.profile = p;
    }
    let flag = |k: &str, d: bool| opts[k].as_bool().unwrap_or(d);
    o.roundtrip = flag("roundtrip", o.roundtrip);
    o.sections = flag("sections", o.sections);
    o.smart_typography = flag("smartTypography", o.smart_typography);
    o.raw_html = flag("rawHtml", o.raw_html);
    o.source_lines = flag("sourceLines", o.source_lines);
    if opts["mode"].as_str() == Some("static") {
        o.mode = Mode::Static;
    }
    o.symbols = map(&opts["symbols"]);
    for (k, v) in map(&opts["labels"]) {
        o.labels.insert(k, v);
    }
    if opts["urlPolicy"]["allow"].is_array() {
        o.url_policy = UrlPolicy::Allow(strings(&opts["urlPolicy"]["allow"]));
    } else if opts["urlPolicy"]["deny"].is_array() {
        o.url_policy = UrlPolicy::Deny(strings(&opts["urlPolicy"]["deny"]));
    }
    o
}

fn cases() -> Vec<Value> {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/data/extra.json"
    ))
    .unwrap_or_default();
    let mut de = serde_json::Deserializer::from_str(&text);
    de.disable_recursion_limit();
    let v: Value = serde::Deserialize::deserialize(&mut de).unwrap_or(Value::Null);
    v.as_array().cloned().unwrap_or_default()
}

#[test]
fn c2_h_extra_inputs_match_the_oracle() {
    let all = cases();
    assert!(!all.is_empty(), "extra fixture not read");
    let mut failures = Vec::new();
    for c in &all {
        let id = c["id"].as_str().unwrap_or("");
        let Ok(Node::Document(doc)) = serde_json::from_value::<Node>(c["tree"].clone()) else {
            failures.push(format!("{id}: no document"));
            continue;
        };
        let opts = options_of(&c["opts"]);
        let mut stripped = c["tree"].clone();
        common::strip_results(&mut stripped);
        if let Ok(Node::Document(mut again)) = serde_json::from_value::<Node>(stripped) {
            carve2::resolve::resolve(&mut again, &opts);
            let back = serde_json::to_value(Node::Document(again)).unwrap_or_default();
            let want = c.get("expectedTree").unwrap_or(&c["tree"]);
            if common::sorted(&back) != common::sorted(want) {
                failures.push(format!(
                    "=== {id} (resolve)\n{}\n--- expected\n{}\n--- got\n{}",
                    c["input"].as_str().unwrap_or(""),
                    common::sorted(want),
                    common::sorted(&back)
                ));
            }
        }
        let html = carve2::html::render(&doc, &opts);
        let want = c["expected"].as_str().or(c["html"].as_str()).unwrap_or("");
        if html != want {
            failures.push(format!(
                "=== {id}\n{}\n{}\n--- expected\n{want}\n--- got\n{html}",
                c["input"].as_str().unwrap_or(""),
                c["opts"]
            ));
        }
    }
    println!(
        "extra html: {}/{} pass",
        all.len() - failures.len(),
        all.len()
    );
    if let Ok(path) = std::env::var("EXTRA_FAILURES") {
        let _ = std::fs::write(path, failures.join("\n"));
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
