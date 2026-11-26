//! C2-T-010 (carve#6436): `--extensions` and `--extension-options` name the
//! standard extensions of `data/extensions.json` on `render`, `fmt`, `lint`
//! and `ast`.
mod common;

use common::parse_json;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("carve-rs-6436-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn carve(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_carve"))
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}

#[test]
fn c2_t_010_standard_extension_cli_clause_cases() {
    let cases: Vec<Value> =
        parse_json(include_str!("../spec-conformance/clause-cases.json"))["cases"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| {
                c["kind"] == "cli"
                    && c["args"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|a| a == "--extensions" || a == "--extension-options")
            })
            .cloned()
            .collect();
    assert_eq!(cases.len(), 19);
    let mut failures = Vec::new();
    for case in cases {
        let id = case["id"].as_str().unwrap();
        let problem = run_case(&case);
        let held = HELD.iter().any(|(h, _)| *h == id);
        match (problem, held) {
            (Some(p), false) => failures.push(format!("{id}: {p}")),
            (None, true) => failures.push(format!("{id}: held but passes; drop the hold")),
            _ => {}
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Cases of a ruling this engine has not ported yet; a held case that passes fails the run.
const HELD: &[(&str, &str)] = &[];

fn run_case(case: &Value) -> Option<String> {
    let id = case["id"].as_str().unwrap();
    let dir = dir(id);
    for (name, file) in case["files"].as_object().unwrap() {
        std::fs::write(dir.join(name), file["text"].as_str().unwrap()).unwrap();
    }
    let args: Vec<&str> = case["args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    let out = carve(&dir, &args);
    let want = &case["expected"];
    let stderr = String::from_utf8_lossy(&out.stderr);
    if out.status.code().map(i64::from) != want["status"].as_i64() {
        return Some(format!("status {:?}: {stderr}", out.status.code()));
    }
    if let Some(text) = want["stdout"].as_str() {
        let got = String::from_utf8_lossy(&out.stdout);
        if got != text {
            return Some(format!("stdout\n{got}\nwant\n{text}"));
        }
    }
    if let Some(category) = want["stderrCategory"].as_str()
        && !stderr.starts_with(&format!("carve[{category}]:"))
    {
        return Some(format!("stderr {stderr}"));
    }
    None
}

#[test]
fn c2_t_010_every_standard_name_is_accepted_by_every_command() {
    let rows = parse_json(include_str!("../spec-data/extensions.json"));
    let dir = dir("names");
    std::fs::write(
        dir.join("a.crv"),
        ":::: tabs\n::: tab \"A\"\na\n:::\n::::\n",
    )
    .unwrap();
    for row in rows["extensions"].as_array().unwrap() {
        let list = format!("[{}]", row["name"]);
        for command in ["render", "ast", "fmt", "lint"] {
            let out = carve(&dir, &[command, "--extensions", &list, "a.crv"]);
            assert_eq!(
                out.status.code(),
                Some(0),
                "{command} {list}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }
}

#[test]
fn c2_t_010_extension_options_refusals_are_usage_errors() {
    let dir = dir("refusals");
    std::fs::write(dir.join("a.crv"), "a\n").unwrap();
    for (extensions, options) in [
        ("\"carve.tabs\"", None),
        ("[1]", None),
        // The text leaves open options for an extension `--extensions` does not list.
        ("[]", Some(r#"{"carve.tabs":{"mode":"aria"}}"#)),
        (
            r#"["carve.tabs"]"#,
            Some(r#"{"carve.tabs":{"mode":"tabs"}}"#),
        ),
        (
            r#"["carve.tabs"]"#,
            Some(r#"{"carve.tabs":{"size":"css"}}"#),
        ),
        (
            r#"["carve.details"]"#,
            Some(r#"{"carve.details":{"mode":"aria"}}"#),
        ),
        (r#"["carve.tabs"]"#, Some(r#"{"carve.tabs":"aria"}"#)),
        (r#"["carve.tabs"]"#, Some("[]")),
    ] {
        let mut args = vec!["render", "--extensions", extensions];
        if let Some(options) = options {
            args.extend(["--extension-options", options]);
        }
        args.push("a.crv");
        let out = carve(&dir, &args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(
            String::from_utf8_lossy(&out.stderr).starts_with("carve[usage]:"),
            "{args:?}"
        );
    }
}

#[test]
fn c2_t_010_ast_reads_the_standard_matchers() {
    let dir = dir("ast");
    std::fs::write(dir.join("a.crv"), "::: details \"S\"\nx\n:::\n").unwrap();
    let plain = carve(&dir, &["ast", "a.crv"]);
    let with = carve(
        &dir,
        &["ast", "--extensions", r#"["carve.details"]"#, "a.crv"],
    );
    assert!(!String::from_utf8_lossy(&plain.stdout).contains("carve.details"));
    assert!(String::from_utf8_lossy(&with.stdout).contains(r#""name":"carve.details""#));
}

#[test]
fn c2_t_010_help_lists_the_flags() {
    let out = carve(&std::env::temp_dir(), &["lint", "--help"]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("  --extensions json\n"), "{text}");
    assert!(text.contains("  --extension-options json\n"), "{text}");
}

#[test]
fn c2_t_010_a_refused_extension_leaves_the_output_file() {
    let dir = dir("output");
    std::fs::write(dir.join("a.crv"), "a\n").unwrap();
    std::fs::write(dir.join("out.html"), "keep").unwrap();
    let out = carve(
        &dir,
        &[
            "render",
            "--extensions",
            r#"["carve.nope"]"#,
            "-o",
            "out.html",
            "a.crv",
        ],
    );
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(
        std::fs::read_to_string(dir.join("out.html")).unwrap(),
        "keep"
    );
}

#[test]
fn c2_x_072_toc_placement_declares_the_slots_of_its_row() {
    let rows = parse_json(include_str!("../spec-data/extensions.json"));
    let row = rows["extensions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "carve.tocPlacement")
        .unwrap();
    let want: Vec<String> = row["slotOrder"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_string())
        .collect();
    let bundle = carve::api::extensions::toc_placement();
    assert_eq!(bundle.extensions[0].declaration.slots, want);
}

#[test]
fn c2_x_072_glossary_and_term_register_once_in_either_order() {
    let dir = dir("glossary");
    std::fs::write(
        dir.join("a.crv"),
        "See :term[API].\n\n::: glossary\n:: API\n: An interface.\n:::\n",
    )
    .unwrap();
    let mut outputs = Vec::new();
    for list in [
        r#"["carve.glossary"]"#,
        r#"["carve.glossary","carve.term"]"#,
        r#"["carve.term","carve.glossary"]"#,
    ] {
        let out = carve(&dir, &["render", "--extensions", list, "a.crv"]);
        assert_eq!(
            out.status.code(),
            Some(0),
            "{list}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        outputs.push(String::from_utf8_lossy(&out.stdout).into_owned());
    }
    assert!(outputs[0].contains(r##"<a href="#gloss-API" class="term">API</a>"##));
    assert!(outputs.iter().all(|o| *o == outputs[0]));
}

#[test]
fn c2_x_075_quote_locale_and_a_listed_smart_quotes_register_once() {
    let dir = dir("quotes");
    std::fs::write(dir.join("a.crv"), "\"a\" b\n").unwrap();
    for (args, want) in [
        (
            vec![
                "--quote-locale",
                "de",
                "--extensions",
                r#"["carve.smartQuotes"]"#,
            ],
            "<p>„a“ b</p>\n",
        ),
        (
            vec![
                "--quote-locale",
                "fr",
                "--extensions",
                r#"["carve.smartQuotes"]"#,
                "--extension-options",
                r#"{"carve.smartQuotes":{"locale":"de"}}"#,
            ],
            "<p>„a“ b</p>\n",
        ),
    ] {
        let mut all = vec!["render", "--smart-typography"];
        all.extend(args);
        all.push("a.crv");
        let out = carve(&dir, &all);
        assert_eq!(
            out.status.code(),
            Some(0),
            "{all:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&out.stdout), want, "{all:?}");
    }
}

fn render(tag: &str, list: &str, src: &str) -> String {
    let dir = dir(tag);
    std::fs::write(dir.join("a.crv"), src).unwrap();
    let out = carve(&dir, &["render", "--extensions", list, "a.crv"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{list}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

/// carve-rs#4846: the marker name registers the marker alone, no index or glossary transform.
#[test]
fn c2_x_072_a_marker_name_registers_no_bundle() {
    assert_eq!(
        render(
            "index-term",
            r#"["carve.indexTerm"]"#,
            ":index[a]\n\n::: index\n:::\n"
        ),
        "<p><span class=\"index-term\"></span></p>\n<div class=\"index\">\n\n</div>\n"
    );
    assert_eq!(
        render(
            "term",
            r#"["carve.term"]"#,
            ":term[A]\n\n::: glossary\n:: A\n: a\n:::\n"
        ),
        "<p><span class=\"term\">A</span></p>\n<div class=\"glossary\">\n  <dl>\n    <dt>A</dt>\n    <dd>a</dd>\n  </dl>\n</div>\n"
    );
    let both = render(
        "index-both",
        r#"["carve.indexTerm","carve.index"]"#,
        ":index[a]\n\n::: index\n:::\n",
    );
    assert!(
        both.contains(r#"<span id="idx-a-1" class="index-term">"#),
        "{both}"
    );
}

/// carve-rs#4847: a placed table of contents with no entries writes no empty line.
#[test]
fn c2_x_072_an_empty_placed_toc_writes_no_empty_line() {
    let list = r#"["carve.tocPlacement"]"#;
    assert_eq!(
        render("toc-empty", list, "::: toc\n:::\n"),
        "<nav class=\"toc\" aria-label=\"Table of contents\"></nav>\n"
    );
    assert_eq!(
        render("toc-titled", list, "::: toc \"Contents\"\n:::\n"),
        "<nav class=\"toc\" aria-labelledby=\"toc-title-1\">\n  <p class=\"admonition-title\" id=\"toc-title-1\">Contents</p>\n</nav>\n"
    );
}
