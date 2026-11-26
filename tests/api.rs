//! The public library `carve::api`: every call, the typed errors and the
//! JSON boundary.

use carve::api::{
    self, Carve, Channel, EncodeOptions, Error, Extension, ExtensionDeclaration, Input,
    LintOptions, Options, Profile, Tree, WriteOptions, Writer, Written,
};
use serde_json::{Value, json};
use std::sync::Arc;

fn opts() -> Options {
    Options::default()
}

#[test]
fn render_text_and_bytes() {
    let html = api::render("Hi *there*", &opts()).unwrap();
    assert_eq!(html, carve::render("Hi *there*", &opts()));
    let bytes: &[u8] = b"a\xffb";
    let from_bytes = api::render(bytes, &opts()).unwrap();
    assert_eq!(from_bytes, carve::try_render_bytes(bytes, &opts()).unwrap());
}

#[test]
fn render_refuses_an_input_over_the_profile_length() {
    let o = Options {
        profile: Profile::Minimal,
        ..opts()
    };
    let long = "a".repeat(10_001);
    let err = api::render(long.as_str(), &o).unwrap_err();
    assert!(matches!(err, Error::Input { limit: 10_000, .. }));
    assert_eq!(err.spec_name(), "CarveInputError");
    assert_eq!(err.category(), "input-invalid");
    assert_eq!(err.status(), 2);
    assert!(api::parse(long.as_str(), &o).is_err());
    assert!(api::render_with_diagnostics(long.as_str(), &o).is_err());
}

#[test]
fn render_with_diagnostics_reports_parse_entries() {
    let r = api::render_with_diagnostics("{{ missing.crv }}\n", &opts()).unwrap();
    assert!(
        r.diagnostics
            .iter()
            .any(|e| e.code == "include-no-resolver")
    );
    assert_eq!(r.suppressed_diagnostics, 0);
    let entry = &r.diagnostics[0];
    assert_eq!(entry.channel, "parse");
    assert!(entry.range.is_some());
}

#[test]
fn strict_losses_refuse_source_render_but_return_a_diagnostic_record() {
    let o = Options {
        profile: Profile::Minimal,
        strict_losses: true,
        ..opts()
    };
    let err = api::render("# Heading", &o).unwrap_err();
    assert!(matches!(&err, Error::Render { codes } if !codes.is_empty()));
    assert_eq!(err.category(), "render-loss");
    assert_eq!(err.status(), 1);
    let diagnostic = api::render_with_diagnostics("# Heading", &o).unwrap();
    assert_eq!(diagnostic.html, "");
    assert_eq!(diagnostic.suppressed_diagnostics, 0);
    assert!(
        diagnostic
            .diagnostics
            .iter()
            .any(|entry| { entry.channel == "render-loss" && entry.severity == "error" })
    );
}

#[test]
fn strict_diagnostic_render_keeps_unallowed_losses_and_counts_capped_losses() {
    let source = "[a](javascript:x)\n\nb `c`{=latex}\n";
    let o = Options {
        strict_losses: true,
        allow_losses: vec!["destination-denied".into()],
        ..opts()
    };
    let result = api::render_with_diagnostics(source, &o).unwrap();
    assert_eq!(result.html, "");
    assert_eq!(result.suppressed_diagnostics, 0);
    assert!(
        result
            .diagnostics
            .iter()
            .any(|entry| { entry.code == "raw-format-dropped" && entry.severity == "error" })
    );

    let capped = Options {
        strict_losses: true,
        max_losses: 0,
        ..opts()
    };
    let result = api::render_with_diagnostics("[a](javascript:x)\n", &capped).unwrap();
    assert_eq!(result.html, "");
    assert_eq!(result.suppressed_diagnostics, 1);
    assert_eq!(result.diagnostics.len(), 0);
    assert!(matches!(
        api::render("[a](javascript:x)\n", &capped),
        Err(Error::Render { .. })
    ));
}

#[test]
fn parse_encode_decode_round_trip() {
    let tree = api::parse("# Title\n\nText", &opts()).unwrap();
    let json = api::encode_tree(&tree);
    assert_eq!(
        json,
        serde_json::to_string(
            &serde_json::from_str::<Value>(&carve::tree_json("# Title\n\nText", &opts())).unwrap()
        )
        .unwrap()
    );
    let back = api::decode_tree(&json).unwrap();
    assert_eq!(api::encode_tree(&back), json);
    api::validate_tree(&back).unwrap();
    assert_eq!(
        api::render_tree(&back, &opts()).unwrap(),
        api::render("# Title\n\nText", &opts()).unwrap()
    );
    assert_eq!(tree.document().children.len(), 2);
}

#[test]
fn parse_bytes_keeps_input_offsets() {
    let bytes: &[u8] = b"a\xff *b*";
    let tree = api::parse(Input::Bytes(bytes), &opts()).unwrap();
    let arena = carve::Arena::new();
    let engine = carve::try_parse_resolved_bytes_in(bytes, &opts(), &arena).unwrap();
    assert_eq!(
        api::encode_tree(&tree),
        serde_json::to_string(&serde_json::from_str::<Value>(&carve::to_json(&engine)).unwrap())
            .unwrap()
    );
}

#[test]
fn positions_off_drops_pos() {
    let tree = api::parse("*a*", &opts()).unwrap();
    let with = api::encode_tree_with(&tree, EncodeOptions::default());
    let without = api::encode_tree_with(&tree, EncodeOptions { positions: false });
    assert!(with.contains("\"pos\""));
    assert!(!without.contains("\"pos\""));
    api::decode_tree(&without).unwrap();
}

#[test]
fn envelope_round_trip() {
    let tree = api::parse("Text", &opts()).unwrap();
    let envelope = api::encode_envelope(&tree);
    let v: Value = serde_json::from_str(&envelope).unwrap();
    assert_eq!(v["astVersion"], "2.0");
    assert_eq!(v["document"]["type"], "document");
    assert_eq!(
        v["document"],
        serde_json::from_str::<Value>(&api::encode_tree(&tree)).unwrap()
    );
    let back = api::decode_tree(&envelope).unwrap();
    let back: Value = serde_json::from_str(&api::encode_tree(&back)).unwrap();
    let original: Value = serde_json::from_str(&api::encode_tree(&tree)).unwrap();
    assert_eq!(back, original);
}

#[test]
fn decode_refuses_a_bad_tree() {
    let err = api::decode_tree(r#"{"type":"document","astVersion":"9"}"#).unwrap_err();
    assert!(matches!(err, Error::Tree(_)));
    assert_eq!(err.spec_name(), "CarveTreeError");
    assert_eq!(err.category(), "ast-json-invalid");
    assert!(matches!(api::decode_tree("not json"), Err(Error::Tree(_))));
}

#[test]
fn parse_with_diagnostics_gives_the_record() {
    let p = api::parse_with_diagnostics("{{ a.crv }}\n", &opts()).unwrap();
    assert!(
        p.diagnostics
            .iter()
            .any(|e| e.code == "include-no-resolver")
    );
    assert_eq!(p.suppressed_diagnostics, 0);
}

#[test]
fn include_cause_entries_are_capped_at_one_hundred() {
    let source = "{{ a.crv }}\n\n".repeat(105);
    let p = api::parse_with_diagnostics(source.as_str(), &opts()).unwrap();
    let kept = p
        .diagnostics
        .iter()
        .filter(|e| e.code.starts_with("include-"))
        .count();
    assert_eq!(kept, 100);
    assert_eq!(p.suppressed_diagnostics, 5);
}

fn include_options(dir: &std::path::Path) -> Options {
    let resolver = carve::include::fs::resolver(dir).unwrap();
    Options {
        includes: Some(carve::include::Includes::new(resolver)),
        ..opts()
    }
}

fn scratch_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("carve-api-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn include_dependencies_of_an_expanded_tree() {
    let dir = scratch_dir("deps");
    std::fs::write(dir.join("part.crv"), "Included.\n").unwrap();
    let o = include_options(&dir);
    let tree = api::parse("{{ part.crv }}\n\n{{ gone.crv }}\n", &o).unwrap();
    let deps = api::include_dependencies(&tree);
    assert_eq!(deps.len(), 2);
    assert_eq!(deps[0].path, "part.crv");
    assert!(deps[0].file.is_some());
    assert_eq!(deps[1].path, "gone.crv");
    assert!(deps[1].file.is_none());
    assert!(
        api::render("{{ part.crv }}", &o)
            .unwrap()
            .contains("Included.")
    );
}

#[test]
fn lint_returns_the_record_and_validates_options() {
    let entries = api::lint("# A\n\n# A\n", &LintOptions::default()).unwrap();
    assert_eq!(
        entries,
        carve::lint::lint("# A\n\n# A\n", &LintOptions::default())
            .iter()
            .map(api::Entry::from)
            .collect::<Vec<_>>()
    );
    let mut bad = LintOptions::default();
    bad.rules.insert("no-such-rule".into(), "on".into());
    let err = api::lint("x", &bad).unwrap_err();
    assert!(matches!(&err, Error::Option { option, .. } if option == "rules"));
    let mut bad = LintOptions::default();
    bad.rules.insert("duplicate-id".into(), "maybe".into());
    assert!(api::lint("x", &bad).is_err());
    let bad = LintOptions {
        platforms: vec!["gitlab".into()],
        ..LintOptions::default()
    };
    assert!(matches!(api::lint("x", &bad), Err(Error::Option { .. })));
}

#[test]
fn calls_this_engine_lacks_are_unsupported() {
    let parsed = api::parse_with_layout("x", &opts()).unwrap();
    assert_eq!(parsed.layout["version"], 2);
    assert_eq!(parsed.layout["source"], "x");
    assert!(api::import_document("<p>x</p>", &api::ImportOptions::default()).is_ok());
}

struct Upper;

impl Writer for Upper {
    #[allow(clippy::unnecessary_literal_bound)]
    fn target(&self) -> &str {
        "stub"
    }

    fn write(&self, tree: &Tree, _opts: &WriteOptions) -> Result<Written, Error> {
        Ok(Written {
            output: format!("{} nodes", tree.document().children.len()),
            diagnostics: Vec::new(),
        })
    }
}

#[test]
fn a_registered_writer_serves_write_and_format() {
    let mut carve = Carve::new();
    let builtin = vec!["ansi", "carve", "html", "markdown", "plain"];
    assert_eq!(carve.writers().targets(), builtin);
    carve.register_writer(Arc::new(Upper)).unwrap();
    assert_eq!(
        carve.writers().targets(),
        vec!["ansi", "carve", "html", "markdown", "plain", "stub"]
    );
    let tree = api::parse("a\n\nb", &opts()).unwrap();
    let w = WriteOptions::default();
    assert_eq!(carve.write_to("stub", &tree, &w).unwrap().output, "2 nodes");
    assert_eq!(carve.write(&tree, &w).unwrap(), "a\n\nb\n");
    assert_eq!(carve.format("a\n\nb", &w).unwrap(), "a\n\nb\n");
    let again = carve.register_writer(Arc::new(Upper)).unwrap_err();
    assert!(matches!(again, Error::Option { .. }));
}

#[test]
fn the_html_writer_renders_the_tree() {
    let tree = api::parse("*a*", &opts()).unwrap();
    let written = Carve::new()
        .write_to("html", &tree, &WriteOptions::default())
        .unwrap();
    assert_eq!(written.output, api::render("*a*", &opts()).unwrap());
    let markdown = Carve::new().write_to("markdown", &tree, &WriteOptions::default());
    assert_eq!(markdown.unwrap().output, "**a**\n");
    let unknown = Carve::new()
        .write_to("pdf", &tree, &WriteOptions::default())
        .unwrap_err();
    assert!(matches!(unknown, Error::Option { .. }), "{unknown}");
    assert!(
        unknown
            .to_string()
            .contains("known targets: ansi, carve, html, markdown, plain"),
        "{unknown}"
    );
}

#[test]
fn the_html_writer_reports_strict_render_losses_as_errors() {
    let tree = api::parse("[a](javascript:x)", &opts()).unwrap();
    let options = WriteOptions {
        render: Options {
            strict_losses: true,
            ..opts()
        },
        ..WriteOptions::default()
    };
    let written = Carve::new().write_to("html", &tree, &options).unwrap();
    assert_eq!(written.output, "");
    assert!(
        written
            .diagnostics
            .iter()
            .any(|entry| { entry.code == "destination-denied" && entry.severity == "error" })
    );
}

#[test]
fn extension_registration_refuses_a_bad_declaration() {
    let ext = |name: &str, version: &str| Extension {
        declaration: ExtensionDeclaration {
            name: name.into(),
            version: version.into(),
            required: false,
            slots: Vec::new(),
        },
        renderers: Vec::new(),
        transform: None,
    };
    let mut carve = Carve::new();
    carve.register_extension(ext("ex.box", "1")).unwrap();
    for (name, version) in [("box", "1"), ("ex.box", ""), ("ex.box", "1")] {
        let err = carve.register_extension(ext(name, version)).unwrap_err();
        assert!(matches!(err, Error::Option { .. }), "{name} {version}");
    }
    assert_ne!(Channel::Interactive, Channel::BuildTime);
}

fn call(request: &Value) -> Value {
    serde_json::from_str(&api::json::call(&request.to_string())).unwrap()
}

#[test]
fn json_boundary_calls() {
    let html = call(&json!({"call": "render", "source": "*a*"}));
    assert_eq!(html["ok"]["html"], api::render("*a*", &opts()).unwrap());
    let bytes = call(&json!({"call": "render", "sourceBytes": [42, 97, 42]}));
    assert_eq!(bytes["ok"]["html"], html["ok"]["html"]);
    let parsed = call(&json!({"call": "parseWithDiagnostics", "source": "x", "positions": false}));
    assert_eq!(parsed["ok"]["tree"]["type"], "document");
    assert_eq!(parsed["ok"]["suppressedDiagnostics"], 0);
    let tree = parsed["ok"]["tree"].clone();
    let from_tree = call(&json!({"call": "render", "tree": tree}));
    assert_eq!(from_tree["ok"]["html"], "<p>x</p>");
    let encoded = call(&json!({"call": "encodeEnvelope", "tree": tree}));
    let decoded = call(&json!({"call": "decodeTree", "json": encoded["ok"]["json"]}));
    assert_eq!(decoded["ok"]["tree"]["type"], "document");
    assert_eq!(
        call(&json!({"call": "validateTree", "tree": tree}))["ok"],
        json!({})
    );
    let deps = call(&json!({"call": "includeDependencies", "tree": tree}));
    assert_eq!(deps["ok"]["dependencies"], json!([]));
    let lint = call(&json!({"call": "lint", "source": "x", "options": {"rules": {}}}));
    assert!(lint["ok"]["diagnostics"].is_array());
    let upgrade =
        call(&json!({"call": "lint", "source": "# a\n b\n", "options": {"upgrade": true}}));
    assert_eq!(
        upgrade["ok"]["diagnostics"][0]["code"],
        "deprecated-indented-line-under-heading"
    );
    let rendered = call(
        &json!({"call": "renderWithDiagnostics", "source": "x", "options": {
            "headingIds": "ascii", "mode": "static", "profile": "article", "sections": false,
            "labels": {"endnotes": "Notes"}, "symbols": {"x": "y"},
            "social": {"mentionUrl": "/u/{name}"}, "urlPolicy": {"allow": ["https"]},
            "extensions": [], "renderers": {}, "includes": null
        }}),
    );
    assert_eq!(rendered["ok"]["html"], "<p>x</p>");
}

#[test]
fn json_boundary_option_errors() {
    let cases = [
        json!({"call": "render", "source": "x", "options": {"nope": true}}),
        json!({"call": "render", "source": "x", "options": {"alignTables": true}}),
        json!({"call": "parse", "source": "x", "options": {"mode": "static"}}),
        json!({"call": "render", "source": "x", "options": {"mode": "loud"}}),
        json!({"call": "render", "source": "x", "options": {"rawHtml": "yes"}}),
        json!({"call": "render", "source": "x", "options": {"labels": {"unknown": "x"}}}),
        json!({"call": "render", "source": "x", "options": {"social": {"tagUrl": "/{tag}"}}}),
        json!({"call": "render", "source": "x", "options": {"social": {"resolveTag": "f"}}}),
        json!({"call": "render", "source": "x", "options": {"urlPolicy": {"deny": [], "allow": []}}}),
        json!({"call": "render", "source": "x", "options": {"renderers": {"a": 1}}}),
        json!({"call": "render", "source": "x", "options": {"includes": {"root": "/no/such/dir"}}}),
        json!({"call": "lint", "source": "x", "options": {"platforms": ["gitlab"]}}),
        json!({"call": "lint", "source": "x", "options": {"upgrade": "yes"}}),
        json!({"call": "render", "source": "x", "options": {"upgrade": true}}),
        json!({"call": "nothing"}),
        json!({"call": "render"}),
        json!({"call": "render", "sourceBytes": [300]}),
        json!({"call": "validateTree"}),
        json!({"call": "render", "source": "x", "options": []}),
    ];
    for request in cases {
        let out = call(&request);
        assert_eq!(out["error"]["name"], "CarveOptionError", "{request}");
        assert_eq!(out["error"]["category"], "usage");
        assert_eq!(out["error"]["status"], 2);
    }
    assert_eq!(api::json::status(&api::json::call("{")), 2);
    assert_eq!(api::json::status(r#"{"ok":{}}"#), 0);
}

#[test]
fn json_boundary_typed_failures() {
    let tree = call(&json!({"call": "validateTree", "tree": {"type": "document"}}));
    assert_eq!(tree["error"]["category"], "ast-json-invalid");
    let loss = call(&json!({"call": "render", "source": "# H",
        "options": {"profile": "minimal", "strictLosses": true}}));
    assert_eq!(loss["error"]["name"], "CarveRenderError");
    assert_eq!(loss["error"]["status"], 1);
    let tree = call(&json!({"call": "parse", "source": "x"}))["ok"]["tree"].clone();
    for name in ["format", "write"] {
        let out = call(&json!({"call": name, "source": "x", "tree": tree}));
        assert!(out.get("ok").is_some(), "{name}: {out}");
    }
    let out = call(&json!({"call": "importDocument", "source": "x", "tree": tree}));
    assert_eq!(out["error"]["category"], "usage");
    let out = call(&json!({"call": "parseWithLayout", "source": "x", "tree": tree}));
    assert_eq!(out["ok"]["layout"]["source"], "x");
    assert_eq!(out["ok"]["layout"]["version"], 2);
}

#[test]
fn diagnostics_of_malformed_bytes_count_input_offsets() {
    let text = "\u{FFFD}\u{FFFD}\n\n{{ a.crv }}\n";
    let bytes: &[u8] = b"\xff\xfe\n\n{{ a.crv }}\n";
    let from_text = api::parse_with_diagnostics(text, &opts()).unwrap();
    let from_bytes = api::parse_with_diagnostics(bytes, &opts()).unwrap();
    let start = |p: &api::Parsed| p.diagnostics[0].range.unwrap().start_offset;
    assert_eq!(start(&from_text), 8);
    assert_eq!(start(&from_bytes), 4);
    let rendered = api::render_with_diagnostics(bytes, &opts()).unwrap();
    assert_eq!(rendered.diagnostics[0].range.unwrap().start_offset, 4);
}

#[test]
fn json_include_dependencies_take_the_top_level_file() {
    let source = "{{ a.crv }}\n";
    let tree = call(&json!({"call": "parse", "source": source}))["ok"]["tree"].clone();
    let deps = call(&json!({"call": "includeDependencies", "tree": tree, "file": "main.crv"}));
    assert_eq!(
        deps["ok"]["dependencies"],
        json!([{"path": "a.crv", "from": "main.crv", "read": false}])
    );
}
