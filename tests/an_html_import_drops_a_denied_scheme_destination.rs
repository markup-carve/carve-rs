//! An `<a href>` or `<img src>` whose scheme the PART 9 §25 sink denylist
//! blanks (`javascript`, `vbscript`, `data`, `file`) is imported exactly like an
//! empty destination: no link or image node, only the content and any surviving
//! attributes, with one `attribute-dropped` warning (markup-carve/carve#2254).
//!
//! The first test reproduces the shared fixture the spec adds after the engines.

use std::io::Write;
use std::process::{Command, Stdio};

use carve::{html_to_ast, html_to_carve, HtmlImportMode, HtmlImportOptions};

const INPUT: &str = "<p><a href=\"javascript:alert(1)\">click here</a> and <a href=\"Java&#9;Script:alert(1)\" id=\"k\">a named one</a></p>\n<img src=\"data:text/html;base64,PHNjcmlwdD4=\" alt=\"logo\">\n";

const HREF_ROW: &str = "Dropped href with a denied URL scheme on <a>";
const SRC_ROW: &str = "Dropped src with a denied URL scheme on <img>";

fn options(mode: HtmlImportMode) -> HtmlImportOptions {
    HtmlImportOptions {
        mode,
        ..HtmlImportOptions::default()
    }
}

fn rows(html: &str, mode: HtmlImportMode) -> Vec<(String, String, String, String, String)> {
    html_to_carve(html, &options(mode))
        .unwrap()
        .report
        .diagnostics
        .iter()
        .map(|d| {
            (
                d.code.as_str().to_string(),
                d.message.clone(),
                d.severity.as_str().to_string(),
                d.fidelity.as_str().to_string(),
                d.confidence.as_str().to_string(),
            )
        })
        .collect()
}

fn dropped(message: &str) -> (String, String, String, String, String) {
    (
        "attribute-dropped".to_string(),
        message.to_string(),
        "warning".to_string(),
        "dropped".to_string(),
        "exact".to_string(),
    )
}

#[test]
fn the_shared_fixture() {
    let result = html_to_carve(INPUT, &HtmlImportOptions::default()).unwrap();
    assert_eq!(result.value, "click here and [a named one]{#k}\n\nlogo\n");
    assert_eq!(result.report.mode.as_str(), "safe");
    assert_eq!(result.report.adapter.as_str(), "generic");
    assert_eq!(
        rows(INPUT, HtmlImportMode::Safe),
        vec![dropped(HREF_ROW), dropped(HREF_ROW), dropped(SRC_ROW)]
    );
    assert!(result.report.diagnostics.iter().all(|d| d.path.is_some()));

    let tree = html_to_ast(INPUT, &HtmlImportOptions::default()).unwrap();
    let mut actual: serde_json::Value = serde_json::from_str(&carve::to_json(&tree.value)).unwrap();
    strip_locations(&mut actual);
    let expected: serde_json::Value = serde_json::from_str(
        r#"{"type":"document","children":[{"type":"paragraph","children":[{"type":"text","value":"click here and "},{"type":"span","children":[{"type":"text","value":"a named one"}],"attrs":{"id":"k"}}]},{"type":"paragraph","children":[{"type":"text","value":"logo"}]}]}"#,
    )
    .unwrap();
    assert_eq!(actual, expected);
}

fn strip_locations(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Array(items) => items.iter_mut().for_each(strip_locations),
        serde_json::Value::Object(map) => {
            map.remove("pos");
            map.remove("srcByteLength");
            map.values_mut().for_each(strip_locations);
        }
        _ => {}
    }
}

#[test]
fn every_mode_drops_it() {
    for mode in [
        HtmlImportMode::Safe,
        HtmlImportMode::Semantic,
        HtmlImportMode::Roundtrip,
    ] {
        let written = html_to_carve(INPUT, &options(mode)).unwrap().value;
        assert_eq!(
            written, "click here and [a named one]{#k}\n\nlogo\n",
            "{mode:?}"
        );
        assert_eq!(
            rows(INPUT, mode),
            vec![dropped(HREF_ROW), dropped(HREF_ROW), dropped(SRC_ROW)],
            "{mode:?}"
        );
    }
}

/// The scheme is read the way the renderer's sink reads it, so every scheme and
/// spelling the sink blanks is denied here too, OS handlers included.
#[test]
fn every_denied_scheme_and_its_evasions() {
    for href in [
        "javascript:alert(1)",
        "JAVASCRIPT:alert(1)",
        "vbscript:msgbox(1)",
        "data:text/html,x",
        "file:///etc/passwd",
        "ms-msdt:/id PCWDiagnostic",
        "search-ms:query=x",
        "shell:startup",
        " javascript:alert(1)",
        "java\tscript:alert(1)",
        "java\u{7f}script:alert(1)",
        "\u{feff}javascript:alert(1)",
    ] {
        let html = format!("<p><a href=\"{href}\">t</a></p>");
        assert_eq!(
            html_to_carve(&html, &HtmlImportOptions::default())
                .unwrap()
                .value,
            "t\n",
            "{href:?}"
        );
        assert_eq!(rows(&html, HtmlImportMode::Safe), vec![dropped(HREF_ROW)]);
    }
}

#[test]
fn a_title_survives_on_the_span() {
    let html = "<p><a href=\"javascript:x\" title=\"hint\">t</a></p>";
    assert_eq!(
        html_to_carve(html, &HtmlImportOptions::default())
            .unwrap()
            .value,
        "[t]{title=hint}\n"
    );
}

/// BOUND: an allowed scheme, or a colon that does not end a scheme, is kept.
#[test]
fn an_allowed_destination_is_kept() {
    for (html, expected) in [
        (
            "<p><a href=\"https://x.test\">t</a></p>",
            "[t](https://x.test)\n",
        ),
        (
            "<p><a href=\"mailto:a@x.test\">t</a></p>",
            "[t](mailto:a@x.test)\n",
        ),
        (
            "<p><a href=\"a/javascript:b\">t</a></p>",
            "[t](a/javascript:b)\n",
        ),
        ("<p>x <img src=\"a.png\" alt=\"l\"></p>", "x ![l](a.png)\n"),
    ] {
        let result = html_to_carve(html, &HtmlImportOptions::default()).unwrap();
        assert_eq!(result.value, expected, "{html}");
        assert!(result.report.diagnostics.is_empty(), "{html}");
    }
}

#[test]
fn check_loss_fails_for_it() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_carve"))
        .args(["migrate", "--from", "html", "--check-loss"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn carve binary");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(b"<p><a href=\"javascript:alert(1)\">t</a></p>\n")
        .expect("write stdin");
    let out = child.wait_with_output().expect("wait carve binary");
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "t\n");
}
