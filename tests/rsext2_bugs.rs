//! Extension-host and source-line regressions found by the rsext2
//! differential against carve-js `next2`.

use carve::api::json::call;
use serde_json::{Value, json};

fn ok(request: &Value) -> Value {
    let reply: Value = serde_json::from_str(&call(&request.to_string())).unwrap();
    reply
        .get("ok")
        .cloned()
        .unwrap_or_else(|| panic!("{reply}"))
}

#[test]
fn an_extension_host_reads_trees_deeper_than_128_levels() {
    let deep = format!("> {}x{}", "{^".repeat(200), "^}".repeat(200));
    let core = ok(&json!({"call": "render", "source": deep}));
    let hosted = ok(&json!({
        "call": "renderWithDiagnostics",
        "source": deep,
        "options": {"extensions": ["carve.details"]},
    }));
    assert_eq!(hosted["html"], core["html"]);
    assert_eq!(hosted["diagnostics"], json!([]));
}

#[test]
fn the_generated_source_line_replaces_an_authored_one() {
    for (source, expected) in [
        (
            "{data-source-line=9}\npara\n",
            "<p data-source-line=\"1\">para</p>",
        ),
        (
            "{DATA-Source-Line=9}\n# H\n",
            "<h1 id=\"H\" data-source-line=\"1\">H</h1>",
        ),
        (
            "{data-source-line=9}\n::: x\na\n:::\n",
            "<div class=\"x\" data-source-line=\"1\">\n  <p data-source-line=\"3\">a</p>\n</div>",
        ),
    ] {
        let out = ok(&json!({
            "call": "render",
            "source": source,
            "options": {"sourceLines": true, "sections": false},
        }));
        assert_eq!(out["html"], expected, "{source}");
    }
    let off = ok(&json!({"call": "render", "source": "{data-source-line=9}\npara\n"}));
    assert_eq!(off["html"], "<p data-source-line=\"9\">para</p>");
}

#[cfg(feature = "fs")]
#[test]
fn extensions_and_includes_combine() {
    let dir = std::env::temp_dir().join(format!("carve-rs-4352-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("p2.crv"), "*x*\n").unwrap();
    for extensions in [
        json!([]),
        json!(["carve.wiki"]),
        json!(["carve.tabs", "carve.details"]),
    ] {
        let out = ok(&json!({
            "call": "renderWithDiagnostics",
            "source": "{{ p2.crv }}",
            "options": {"includes": {"root": dir.to_str().unwrap()}, "extensions": extensions},
        }));
        assert_eq!(out["html"], "<p><strong>x</strong></p>", "{extensions}");
        assert_eq!(out["diagnostics"], json!([]), "{extensions}");
    }
}

#[test]
fn an_authored_title_on_a_nested_semantic_span_stays_on_the_outermost_element() {
    for (source, expected) in [
        (
            "[x]{abbr=A kbd title=T}",
            "<p><kbd title=\"T\"><abbr title=\"A\">x</abbr></kbd></p>",
        ),
        (
            "[x]{time=2020 abbr=A title=T datetime=D}",
            "<p><time title=\"T\" datetime=\"D\"><abbr title=\"A\">x</abbr></time></p>",
        ),
        (
            "[x]{abbr=A data-x=1 title=T}",
            "<p><abbr data-x=\"1\" title=\"T\">x</abbr></p>",
        ),
        ("[x]{abbr=A Title=T}", "<p><abbr Title=\"T\">x</abbr></p>"),
        (
            "[x]{data-x=1 abbr=A}",
            "<p><abbr title=\"A\" data-x=\"1\">x</abbr></p>",
        ),
    ] {
        assert_eq!(
            ok(&json!({"call": "render", "source": source}))["html"],
            expected,
            "{source}"
        );
    }
}
