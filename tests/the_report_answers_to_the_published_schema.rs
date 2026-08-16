//! The HTML import report is a wire format: the spec's HTML import contract
//! fixes its shape and `resources/html-import-schema.json` is the
//! machine-readable copy of that. Nothing in this crate used to read that file - the code
//! spellings existed twice (once in the enum, once in the CLI's serializer)
//! and no test compared either to the schema, so renaming one to
//! `BOGUS-NOT-IN-SCHEMA` left the suite green. The shared HTML import fixtures
//! could not have caught it either: they only ever produce `element-dropped`
//! and `attribute-dropped`, so seven of the nine codes were never written at
//! all.
//!
//! So this holds the crate to the schema from both ends:
//!
//!   - every vocabulary - mode, adapter, diagnostic code, severity - is
//!     compared to the schema's enum as a SET, in both directions. The lists
//!     come from each type's own `ALL`, which the `report_vocabulary!` macro
//!     generates from the same table as the variants, so a variant cannot be
//!     added without appearing here.
//!   - a report carrying each code in turn, and the real report the CLI
//!     writes, are validated against the schema as documents - shape and all,
//!     not just the vocabulary.
//!
//! The validator is small enough to live here and covers exactly the keywords
//! the schema uses; it panics rather than shrugging when it meets one it does
//! not know, and the control cases below prove it actually rejects.

use carve::{HtmlImportAdapter, HtmlImportDiagnosticCode, HtmlImportMode, HtmlImportSeverity};
use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn schema_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/spec/resources/html-import-schema.json")
}

/// The pinned schema. A missing or unreadable file FAILS here: a check that
/// skips itself when it cannot find its own oracle is no check.
fn schema() -> Value {
    let path = schema_path();
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "cannot read {}: {error}\nthe spec submodule is missing - run `git submodule update --init`",
            path.display()
        )
    });
    serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("{} is not JSON: {error}", path.display()))
}

/// The spellings the schema admits at `pointer`, sorted.
fn admitted(schema: &Value, pointer: &str) -> Vec<String> {
    let node = schema
        .pointer(pointer)
        .unwrap_or_else(|| panic!("{pointer} is missing from html-import-schema.json"));
    let values = node
        .as_array()
        .unwrap_or_else(|| panic!("{pointer} is not an enum array"));
    assert!(!values.is_empty(), "{pointer} admits nothing");
    let mut names = values
        .iter()
        .map(|v| {
            v.as_str()
                .unwrap_or_else(|| panic!("{pointer} holds a non-string member {v}"))
                .to_string()
        })
        .collect::<Vec<_>>();
    names.sort();
    names
}

fn sorted(names: impl IntoIterator<Item = &'static str>) -> Vec<String> {
    let mut names = names.into_iter().map(str::to_string).collect::<Vec<_>>();
    names.sort();
    names
}

/// Every keyword the schema is allowed to use. A schema that gains one this
/// validator does not implement would otherwise be silently under-enforced,
/// so meeting an unknown keyword is a failure, not a shrug.
const KNOWN_KEYWORDS: &[&str] = &[
    "$schema",
    "$id",
    "title",
    "description",
    "type",
    "enum",
    "required",
    "properties",
    "additionalProperties",
    "items",
    "minimum",
];

/// Validate `instance` against `schema`, collecting human-readable failures.
fn validate(schema: &Value, instance: &Value, path: &str, errors: &mut Vec<String>) {
    let object = schema
        .as_object()
        .unwrap_or_else(|| panic!("schema node at {path} is not an object"));
    for keyword in object.keys() {
        assert!(
            KNOWN_KEYWORDS.contains(&keyword.as_str()),
            "html-import-schema.json uses `{keyword}` at {path}, which this validator does not implement"
        );
    }

    if let Some(allowed) = object.get("enum").and_then(Value::as_array) {
        if !allowed.contains(instance) {
            errors.push(format!("{path}: {instance} is not one of {allowed:?}"));
        }
        return;
    }

    let kind = object
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_else(|| {
            panic!("schema node at {path} constrains nothing (no `type`, no `enum`)")
        });
    match kind {
        "object" => {
            let Some(map) = instance.as_object() else {
                errors.push(format!("{path}: expected an object, got {instance}"));
                return;
            };
            let properties = object.get("properties").and_then(Value::as_object);
            for name in object
                .get("required")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let name = name.as_str().expect("required names are strings");
                if !map.contains_key(name) {
                    errors.push(format!("{path}: required property `{name}` is missing"));
                }
            }
            for (name, value) in map {
                match properties.and_then(|p| p.get(name)) {
                    Some(sub) => validate(sub, value, &format!("{path}/{name}"), errors),
                    None => {
                        if object.get("additionalProperties") == Some(&Value::Bool(false)) {
                            errors.push(format!("{path}: property `{name}` is not allowed"));
                        }
                    }
                }
            }
        }
        "array" => {
            let Some(items) = instance.as_array() else {
                errors.push(format!("{path}: expected an array, got {instance}"));
                return;
            };
            if let Some(sub) = object.get("items") {
                for (i, value) in items.iter().enumerate() {
                    validate(sub, value, &format!("{path}/{i}"), errors);
                }
            }
        }
        "string" => {
            if !instance.is_string() {
                errors.push(format!("{path}: expected a string, got {instance}"));
            }
        }
        "integer" => {
            let Some(n) = instance.as_i64() else {
                errors.push(format!("{path}: expected an integer, got {instance}"));
                return;
            };
            if let Some(min) = object.get("minimum").and_then(Value::as_i64) {
                if n < min {
                    errors.push(format!("{path}: {n} is below the minimum {min}"));
                }
            }
        }
        other => panic!("schema node at {path} uses the unimplemented type `{other}`"),
    }
}

fn errors_for(schema: &Value, instance: &Value) -> Vec<String> {
    let mut errors = Vec::new();
    validate(schema, instance, "", &mut errors);
    errors
}

/// Both directions, as sets: a spelling the crate can write that the schema
/// does not admit fails, and an admitted spelling the crate cannot write fails
/// too (the schema moved and nobody told the engine).
#[test]
fn every_report_vocabulary_is_exactly_the_one_the_schema_admits() {
    let schema = schema();
    let ours = [
        (
            "/properties/mode/enum",
            sorted(HtmlImportMode::ALL.iter().map(|v| v.as_str())),
        ),
        (
            "/properties/adapter/enum",
            sorted(HtmlImportAdapter::ALL.iter().map(|v| v.as_str())),
        ),
        (
            "/properties/diagnostics/items/properties/code/enum",
            sorted(HtmlImportDiagnosticCode::ALL.iter().map(|v| v.as_str())),
        ),
        (
            "/properties/diagnostics/items/properties/severity/enum",
            sorted(HtmlImportSeverity::ALL.iter().map(|v| v.as_str())),
        ),
    ];
    for (pointer, ours) in ours {
        assert_eq!(
            admitted(&schema, pointer),
            ours,
            "{pointer}: the schema and this crate disagree about the vocabulary"
        );
    }
}

/// The vocabulary check compares names; this one writes each name into a real
/// report and puts the document through the schema. It reaches the seven codes
/// no shared fixture produces, which is where the hole was.
#[test]
fn a_report_carrying_any_single_code_validates() {
    let schema = schema();
    assert_eq!(
        HtmlImportDiagnosticCode::ALL.len(),
        admitted(
            &schema,
            "/properties/diagnostics/items/properties/code/enum"
        )
        .len(),
        "the crate writes a different number of codes than the schema admits"
    );
    for code in HtmlImportDiagnosticCode::ALL {
        for severity in HtmlImportSeverity::ALL {
            let report = json!({
                "mode": HtmlImportMode::Safe.as_str(),
                "adapter": HtmlImportAdapter::Generic.as_str(),
                "diagnostics": [{
                    "code": code.as_str(),
                    "message": "sample",
                    "severity": severity.as_str(),
                    "path": "/html[1]/body[1]",
                }],
            });
            let errors = errors_for(&schema, &report);
            assert!(
                errors.is_empty(),
                "{}/{} does not validate: {errors:?}",
                code.as_str(),
                severity.as_str()
            );
        }
    }
    for mode in HtmlImportMode::ALL {
        for adapter in HtmlImportAdapter::ALL {
            let report = json!({
                "mode": mode.as_str(),
                "adapter": adapter.as_str(),
                "diagnostics": [],
            });
            let errors = errors_for(&schema, &report);
            assert!(
                errors.is_empty(),
                "{}/{} does not validate: {errors:?}",
                mode.as_str(),
                adapter.as_str()
            );
        }
    }
}

/// The bytes a user actually gets. `--report -` writes the JSON to stderr.
#[test]
fn the_report_the_cli_writes_validates() {
    let schema = schema();
    // Between them these lose several different things: a dropped element, a
    // dropped attribute, an unwrapped element and an unmapped style, under two
    // modes and two adapters so those vocabularies are written too.
    let samples: [(&[&str], &str); 3] = [
        (
            &["--mode", "safe"],
            concat!(
                "<p onclick=\"x()\">text</p>\n",
                "<script>drop me</script>\n",
                "<blink>gone</blink>\n",
                "<p style=\"color: red\">styled</p>\n",
            ),
        ),
        (
            &["--mode", "roundtrip", "--adapter", "word"],
            "<p class=\"MsoNormal\"><o:p>x</o:p>kept</p>\n",
        ),
        (
            &["--mode", "semantic", "--adapter", "google-docs"],
            "<p><span style=\"font-weight:700\">bold</span><iframe src=\"x\"></iframe></p>\n",
        ),
    ];
    let mut seen = 0usize;
    for (args, html) in samples {
        let mut child = Command::new(env!("CARGO_BIN_EXE_carve"))
            .args(["migrate", "--from", "html", "--report", "-"])
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn carve binary");
        let mut stdin = child.stdin.take().expect("stdin");
        stdin.write_all(html.as_bytes()).expect("write stdin");
        drop(stdin);
        let out = child.wait_with_output().expect("wait carve binary");
        assert!(out.status.success(), "carve migrate {args:?} failed");
        let report_text = String::from_utf8(out.stderr).expect("utf8 stderr");
        let report: Value = serde_json::from_str(report_text.trim())
            .unwrap_or_else(|error| panic!("report is not JSON: {error}\n{report_text}"));
        seen += report
            .pointer("/diagnostics")
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or(0);
        let errors = errors_for(&schema, &report);
        assert!(errors.is_empty(), "{args:?}\n{report_text}\n{errors:?}");
    }
    // A report with nothing in it would pass the validator while proving
    // nothing about the diagnostics, so the samples have to actually lose
    // something.
    assert!(seen > 0, "the samples lost nothing");
}

/// The validator's own control cases. Without these, "no errors" could just
/// mean the validator never looks.
#[test]
fn the_validator_rejects_what_the_schema_forbids() {
    let schema = schema();
    let valid = json!({
        "mode": "safe",
        "adapter": "generic",
        "diagnostics": [{
            "code": "element-dropped",
            "message": "dropped <blink>",
            "severity": "warning",
        }],
    });
    assert!(
        errors_for(&schema, &valid).is_empty(),
        "the control's baseline must validate"
    );

    let mut unknown_code = valid.clone();
    unknown_code["diagnostics"][0]["code"] = json!("BOGUS-NOT-IN-SCHEMA");
    let errors = errors_for(&schema, &unknown_code);
    assert!(
        errors.iter().any(|e| e.contains("BOGUS-NOT-IN-SCHEMA")),
        "an unadmitted code must be rejected: {errors:?}"
    );

    let mut extra_property = valid.clone();
    extra_property["diagnostics"][0]["hint"] = json!("try harder");
    let errors = errors_for(&schema, &extra_property);
    assert!(
        errors.iter().any(|e| e.contains("`hint` is not allowed")),
        "a property the schema does not declare must be rejected: {errors:?}"
    );

    let mut missing_required = valid.clone();
    missing_required["diagnostics"][0]
        .as_object_mut()
        .expect("diagnostic object")
        .remove("severity");
    let errors = errors_for(&schema, &missing_required);
    assert!(
        errors.iter().any(|e| e.contains("`severity` is missing")),
        "a missing required property must be rejected: {errors:?}"
    );

    let mut wrong_type = valid.clone();
    wrong_type["diagnostics"][0]["line"] = json!(0);
    let errors = errors_for(&schema, &wrong_type);
    assert!(
        errors.iter().any(|e| e.contains("below the minimum")),
        "a line number below the minimum must be rejected: {errors:?}"
    );
}

/// The names the CLI takes for `--mode` and `--adapter` are the names the
/// report writes back, so a flag cannot introduce a value the schema refuses.
#[test]
fn the_cli_reads_back_every_name_the_report_writes() {
    for mode in HtmlImportMode::ALL {
        assert_eq!(HtmlImportMode::from_name(mode.as_str()), Some(*mode));
    }
    for adapter in HtmlImportAdapter::ALL {
        assert_eq!(
            HtmlImportAdapter::from_name(adapter.as_str()),
            Some(*adapter)
        );
    }
    assert_eq!(HtmlImportMode::from_name("BOGUS-NOT-IN-SCHEMA"), None);
    assert_eq!(HtmlImportAdapter::from_name("BOGUS-NOT-IN-SCHEMA"), None);
}
