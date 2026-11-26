//! C2-T-020, C2-N-087: a render loss in an included file carries
//! `includedBy`, the chain of its first reach (carve-rs#4862).

use carve::api::{self, Cause, Includes, Options, Request};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;

fn includes(files: &[(&str, &str)]) -> Includes {
    let sources: HashMap<String, Vec<u8>> = files
        .iter()
        .map(|(name, text)| ((*name).to_string(), text.as_bytes().to_vec()))
        .collect();
    Includes::new(Arc::new(move |request: &Request<'_>| {
        sources
            .get(request.path)
            .cloned()
            .map(|bytes| (bytes, request.path.to_string()))
            .ok_or(Cause::NotFound)
    }))
}

fn losses(source: &str, files: &[(&str, &str)], options: Options) -> Vec<(String, Value)> {
    let options = Options {
        includes: Some(includes(files)),
        ..options
    };
    api::render_with_diagnostics(source, &options)
        .unwrap()
        .diagnostics
        .iter()
        .filter(|e| e.channel == "render-loss")
        .map(|e| {
            let v = serde_json::to_value(e).unwrap();
            (format!("{} {}", e.code, v["file"]), v["includedBy"].clone())
        })
        .collect()
}

const LATEX: &str = "``` =latex\n\\x\n```\n";

#[test]
fn a_raw_loss_two_levels_down_carries_both_sites() {
    let got = losses(
        "top\n\n{{ n.crv }}\n",
        &[("n.crv", "{{ m.crv }}\n"), ("m.crv", LATEX)],
        Options::default(),
    );
    assert_eq!(
        got,
        vec![(
            "raw-format-dropped \"m.crv\"".into(),
            json!([
                {"range": {"startOffset": 5, "endOffset": 16}},
                {"file": "n.crv", "range": {"startOffset": 0, "endOffset": 11}}
            ])
        )]
    );
}

#[test]
fn a_position_reached_twice_carries_the_chain_of_the_first_reach() {
    let got = losses(
        "{{ m.crv }}\n\n{{ n.crv }}\n",
        &[("n.crv", "{{ m.crv }}\n"), ("m.crv", LATEX)],
        Options::default(),
    );
    let first = json!([{"range": {"startOffset": 0, "endOffset": 11}}]);
    assert_eq!(
        got,
        vec![
            ("raw-format-dropped \"m.crv\"".into(), first.clone()),
            ("raw-format-dropped \"m.crv\"".into(), first),
        ]
    );
}

#[test]
fn a_carrier_loss_in_an_included_file_carries_the_chain() {
    let got = losses(
        "{{ n.crv }}\n",
        &[
            ("n.crv", "x\n\n{{ m.crv }}\n"),
            ("m.crv", "{data-marker=1}\npara\n"),
        ],
        Options::default(),
    );
    assert_eq!(
        got,
        vec![(
            "carrier-attribute-dropped \"m.crv\"".into(),
            json!([
                {"range": {"startOffset": 0, "endOffset": 11}},
                {"file": "n.crv", "range": {"startOffset": 3, "endOffset": 14}}
            ])
        )]
    );
}

#[test]
fn a_heading_shift_loss_in_an_included_file_carries_the_chain() {
    let got = losses(
        "x\n\n{{ m.crv }}\n",
        &[("m.crv", "###### H\n")],
        Options {
            heading_shift: 1,
            ..Options::default()
        },
    );
    assert_eq!(
        got,
        vec![(
            "heading-shift-above-6 \"m.crv\"".into(),
            json!([{"range": {"startOffset": 3, "endOffset": 14}}])
        )]
    );
}

#[test]
fn a_loss_in_the_top_level_document_has_no_chain() {
    let got = losses(
        "{{ n.crv }}\n\n``` =latex\n\\x\n```\n",
        &[("n.crv", "x\n")],
        Options::default(),
    );
    assert_eq!(got, vec![("raw-format-dropped null".into(), Value::Null)]);
}

#[test]
fn the_command_line_adds_line_and_column_to_each_site() {
    let dir = std::env::temp_dir().join(format!("carve-render-loss-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("o.crv"), "top\n\n{{ n.crv }}\n").unwrap();
    std::fs::write(dir.join("n.crv"), "{{ m.crv }}\n").unwrap();
    std::fs::write(dir.join("m.crv"), LATEX).unwrap();
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_carve"))
        .args(["o.crv", "--to", "plain", "--report-losses", "losses.json"])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(status.status.success());
    let report: Value =
        serde_json::from_slice(&std::fs::read(dir.join("losses.json")).unwrap()).unwrap();
    let entry = &report["files"][0]["diagnostics"][0];
    assert_eq!(entry["code"], "raw-format-dropped");
    assert_eq!(entry["file"], "m.crv");
    assert_eq!(
        entry["includedBy"],
        json!([
            {"line": 3, "column": 1, "range": {"startOffset": 5, "endOffset": 16}},
            {"file": "n.crv", "line": 1, "column": 1, "range": {"startOffset": 0, "endOffset": 11}}
        ])
    );
}
