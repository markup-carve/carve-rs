//! The `import` entries of the shared corpus (`spec-conformance/`) through
//! `carve::api::import_document`, compared as `conformance.rs` compares
//! trees and records. The `carve` source of an entry is not compared: the
//! source comes from the `carve` writer, which this engine does not have yet.
mod common;

use carve::api::{self, Entry, Error, ImportOptions, Imported};
use common::{parse_json, sorted};
use serde_json::{Value, json};

/// `(entry id, checked field)` held, each with the clause or cause.
const HELD: &[(&str, &str)] = &[
    // carve#6771, #6794, #6795 (carve-rs lines): the listTable import option.
    ("CC-2156", "children"),
    ("CC-2159", "children"),
    ("CC-2160", "children"),
    ("CC-2161", "children"),
    ("CC-2162", "children"),
    ("CC-2163", "children"),
    ("CC-2177", "children"),
    ("CC-2178", "children"),
    ("CC-2179", "children"),
    ("CC-2180", "children"),
    ("CC-2181", "children"),
    ("CC-2182", "children"),
    ("CC-2183", "children"),
    ("CC-2184", "children"),
    ("CC-2161", "diagnostic:import-structure-repaired"),
    ("CC-2163", "diagnostic:import-attribute-dropped"),
    // C2-H-204: `allowLosses` of the importer needs a list on `ImportOptions`,
    // a `Copy` type whose shape this round keeps.
    ("CC-1260", "imported"),
    // carve#6804: an imported image whose alt is unknown.
    ("CC-2207", "children"),
    ("CC-2207", "diagnostic:image-alt-unknown"),
    ("CC-2208", "children"),
    // carve#6605, carve#6235 (carve#6812 G2): HTML import.
    ("CC-2230", "children"),
    ("CC-2230", "notReported:import-attribute-dropped"),
    ("CC-2231", "children"),
    ("CC-2231", "notReported:import-attribute-dropped"),
    ("CC-2232", "children"),
    ("CC-2232", "notReported:import-attribute-dropped"),
    ("CC-2233", "children"),
    ("CC-2234", "children"),
    ("CC-2235", "diagnostic:import-element-dropped"),
    ("CC-2236", "children"),
    // carve#6623 (carve#6812 G1): th rows next to a scope row th row.
    ("CC-2063", "diagnostic:field-unspellable"),
    ("CC-2250", "diagnostic:field-unspellable"),
];

/// The import entries that carry an expected `carve` source (not compared).
const WITH_CARVE_CLAUSE: &[&str] = &[
    "CC-907", "CC-1044", "CC-1260", "CC-1419", "CC-1420", "CC-1421", "CC-1422", "CC-1423",
    "CC-1424", "CC-1425", "CC-1426", "CC-1427", "CC-1428", "CC-1429", "CC-1430", "CC-1431",
    "CC-1432", "CC-1433", "CC-1434", "CC-1450", "CC-1451", "CC-1452", "CC-1453", "CC-1454",
    "CC-1455", "CC-1456", "CC-1525", "CC-1526", "CC-1527", "CC-1528", "CC-1529", "CC-1530",
    "CC-2010", "CC-2011", "CC-2046", "CC-2047", "CC-2048", "CC-2049", "CC-2050", "CC-2051",
    "CC-2052", "CC-2053", "CC-2054", "CC-2055", "CC-2056", "CC-2057", "CC-2058", "CC-2059",
    "CC-2060", "CC-2061", "CC-2062", "CC-2063", "CC-2064", "CC-2073", "CC-2074", "CC-2075",
    "CC-2076", "CC-2077", "CC-2078", "CC-2100", "CC-2101", "CC-2102", "CC-2103", "CC-2104",
    "CC-2107", "CC-2108", "CC-2109", "CC-2110", "CC-2149", "CC-2150", "CC-2151", "CC-2152",
    "CC-2153", "CC-2156", "CC-2159", "CC-2160", "CC-2161", "CC-2162", "CC-2163", "CC-2167",
    "CC-2168", "CC-2169", "CC-2170", "CC-2171", "CC-2177", "CC-2178", "CC-2179", "CC-2180",
    "CC-2181", "CC-2182", "CC-2183", "CC-2184", "CC-2197", "CC-2198", "CC-2199", "CC-2200",
    "CC-2201", "CC-2202", "CC-2203", "CC-2204", "CC-2207", "CC-2208", "CC-2209", "CC-2220",
    "CC-2229", "CC-2230", "CC-2231", "CC-2232", "CC-2233", "CC-2234", "CC-2235", "CC-2236",
    "CC-2244", "CC-2245", "CC-2250", "CC-2252", "CC-2253", "CC-2254", "CC-2255", "CC-2256",
    "CC-2257", "CC-2394",
];

fn load(name: &str) -> Value {
    let path = format!("{}/spec-conformance/{name}", env!("CARGO_MANIFEST_DIR"));
    parse_json(&std::fs::read_to_string(path).unwrap_or_default())
}

fn options(v: &Value) -> Result<ImportOptions, Error> {
    // C2-M-102: an unknown adapter is an option error before the import starts.
    if let Some(name) = v["adapter"].as_str() {
        ImportOptions::from_json(Some(&json!({"from": "html", "adapter": name})))?;
    }
    Ok(ImportOptions {
        raw_html: v["rawHtml"].as_bool().unwrap_or(false),
        strict_losses: v["strictLosses"].as_bool().unwrap_or(false),
        adapter: v["adapter"]
            .as_str()
            .and_then(api::ImportAdapter::from_name)
            .unwrap_or_default(),
    })
}

/// C2-T-019: LINE counts line feeds from 1, COLUMN the code points of the line.
fn line_column(input: &str, offset: usize) -> (u64, u64) {
    let before = input.get(..offset).unwrap_or(input);
    let line = before.matches('\n').count() as u64 + 1;
    let start = before.rfind('\n').map_or(0, |i| i + 1);
    let column = before.get(start..).unwrap_or("").chars().count() as u64 + 1;
    (line, column)
}

fn agrees(want: &Value, got: &Entry, input: &str) -> bool {
    let code = want.as_str().or_else(|| want["code"].as_str());
    let at = got.range.map(|r| line_column(input, r.start_offset));
    code == Some(got.code.as_str())
        && want["channel"].as_str().is_none_or(|v| v == got.channel)
        && want["severity"].as_str().is_none_or(|v| v == got.severity)
        && want["line"]
            .as_u64()
            .is_none_or(|v| at.is_some_and(|a| a.0 == v))
        && want["column"]
            .as_u64()
            .is_none_or(|v| at.is_some_and(|a| a.1 == v))
}

fn children(imported: &Imported) -> Value {
    parse_json(&api::encode_tree_with(
        &imported.tree,
        api::EncodeOptions { positions: false },
    ))["children"]
        .clone()
}

#[derive(Default)]
struct Run {
    pass: usize,
    held: usize,
    failures: Vec<String>,
}

impl Run {
    fn check(&mut self, id: &str, field: &str, ok: bool, detail: impl FnOnce() -> String) {
        let held = HELD.iter().any(|&(h, f)| h == id && f == field);
        match (ok, held) {
            (true, false) => self.pass += 1,
            (false, true) => self.held += 1,
            (true, true) => self
                .failures
                .push(format!("=== {id} {field}: held but passes; drop the hold")),
            (false, false) => self
                .failures
                .push(format!("=== {id} {field}\n{}", detail())),
        }
    }

    fn finish(self, name: &str) {
        println!(
            "{name}: {} checks pass, {} held, {} fail",
            self.pass,
            self.held,
            self.failures.len()
        );
        assert!(self.failures.is_empty(), "{}", self.failures.join("\n"));
    }
}

fn check_entry(
    run: &mut Run,
    id: &str,
    html: &str,
    opts: Result<ImportOptions, Error>,
    expected: &Value,
) {
    let result = opts.and_then(|opts| api::import_document(html, &opts));
    if let Some(want) = expected.get("refusal") {
        let name = want["error"].as_str().unwrap_or("");
        let got = result.as_ref().err().map(Error::spec_name);
        run.check(id, "refusal", got == Some(name), || {
            format!("expected {name}; got {:?}", result.as_ref().err())
        });
        return;
    }
    let imported = match result {
        Ok(i) => i,
        Err(e) => {
            run.check(id, "imported", false, || format!("refused: {e}"));
            return;
        }
    };
    if let Some(want) = expected.get("children") {
        let got = children(&imported);
        let (want, got) = (sorted(want), sorted(&got));
        run.check(id, "children", want == got, || {
            format!("{html}\n--- expected\n{want}\n--- got\n{got}")
        });
    }
    for d in expected["diagnostics"].as_array().into_iter().flatten() {
        let code = d.as_str().or_else(|| d["code"].as_str()).unwrap_or("");
        run.check(
            id,
            &format!("diagnostic:{code}"),
            imported.diagnostics.iter().any(|g| agrees(d, g, html)),
            || format!("{html}\nexpected {d}; got {:?}", imported.diagnostics),
        );
    }
    for code in expected["notReported"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        run.check(
            id,
            &format!("notReported:{code}"),
            !imported.diagnostics.iter().any(|g| g.code == code),
            || format!("{html}\nunexpected {code}; got {:?}", imported.diagnostics),
        );
    }
}

#[test]
fn c2_m_import_clause_cases() {
    let corpus = load("clause-cases.json");
    let mut run = Run::default();
    let mut with_carve = Vec::new();
    for c in corpus["cases"].as_array().into_iter().flatten() {
        if c["kind"] != "import" {
            continue;
        }
        let id = c["id"].as_str().unwrap_or("");
        if c["expected"].get("carve").is_some() {
            with_carve.push(id.to_string());
        }
        let html = c["html"].as_str().unwrap_or("");
        check_entry(&mut run, id, html, options(&c["options"]), &c["expected"]);
    }
    println!("clause cases expecting carve source (not compared): {with_carve:?}");
    assert_eq!(with_carve, WITH_CARVE_CLAUSE);
    run.finish("import-clause-cases");
}

#[test]
fn c2_m_import_examples() {
    let corpus = load("examples.json");
    let mut run = Run::default();
    let mut with_carve = 0;
    for e in corpus["examples"].as_array().into_iter().flatten() {
        if e["kind"] != "import" {
            continue;
        }
        let id = e["id"].as_str().unwrap_or("");
        if e["expected"].get("carve").is_some() {
            with_carve += 1;
        }
        let html = e["html"].as_str().unwrap_or("");
        check_entry(&mut run, id, html, options(&e["options"]), &e["expected"]);
    }
    println!("examples expecting carve source (not compared): {with_carve}");
    run.finish("import-examples");
}

#[test]
fn c2_m_082_strict_losses_is_a_render_error() {
    let strict = ImportOptions {
        strict_losses: true,
        ..ImportOptions::default()
    };
    let err = api::import_document("<p onclick=\"x()\">a</p>", &strict).unwrap_err();
    assert!(matches!(&err, Error::Render { codes } if codes == &["import-attribute-dropped"]));
    assert_eq!(err.category(), "render-loss");
    assert!(api::import_document("<p>a</p>", &strict).is_ok());
}

#[test]
fn c2_m_081_entries_carry_byte_ranges_and_foreign_types() {
    let html = "<p>\u{e9}</p>\n<p onclick=\"go()\">b</p>";
    let out = api::import_document(html, &ImportOptions::default()).unwrap();
    let entry = &out.diagnostics[0];
    assert_eq!(entry.foreign_type.as_deref(), Some("p"));
    let range = entry.range.unwrap();
    assert_eq!(
        &html[range.start_offset..range.end_offset],
        "<p onclick=\"go()\">b</p>"
    );
}

#[test]
fn c2_m_006_deep_input_is_refused() {
    let html = format!("{}a{}", "<div>".repeat(700), "</div>".repeat(700));
    let err = api::import_document(&html, &ImportOptions::default()).unwrap_err();
    assert_eq!(err.spec_name(), "ImportInputError");
    assert_eq!(err.category(), "import-input-invalid");
}

#[test]
fn import_options_from_json() {
    let ok = ImportOptions::from_json(Some(&json!({"from": "html", "rawHtml": true}))).unwrap();
    assert!(ok.raw_html && !ok.strict_losses);
    for bad in [
        None,
        Some(json!({})),
        Some(json!({"from": "bbcode"})),
        Some(json!({"from": 1})),
        Some(json!({"from": "html", "rawHtml": "yes"})),
        Some(json!({"from": "html", "strictLosses": 1})),
        Some(json!({"from": "html", "profile": "article"})),
        Some(json!("html")),
    ] {
        let err = ImportOptions::from_json(bad.as_ref()).unwrap_err();
        assert!(matches!(err, Error::Option { .. }), "{bad:?}: {err}");
        assert_eq!(err.category(), "usage");
    }
}
