use carve::{
    lint_carve, lint_carve_with_options, IncludeContext, IncludeDenial, IncludeOptions,
    IncludeResolved, Options,
};
use serde_json::{json, Value};
use std::collections::HashMap;

#[test]
fn all_lint_include_vectors() {
    let corpus: Value = serde_json::from_str(include_str!(
        "spec/tests/lint-include-conformance/vectors.json"
    ))
    .unwrap();
    let vectors = corpus["vectors"].as_array().unwrap();
    assert_eq!(vectors.len(), 23);
    for vector in vectors {
        let files: HashMap<String, String> =
            serde_json::from_value(vector["files"].clone()).unwrap();
        let resolver =
            |path: &str, _: &IncludeContext<'_>| -> Result<IncludeResolved, IncludeDenial> {
                files
                    .get(path)
                    .cloned()
                    .map(IncludeResolved::from)
                    .ok_or(IncludeDenial::Unresolved)
            };
        let mut options = Options::new();
        if vector["resolver"] == "virtual" {
            options = options.with_lint_includes(
                IncludeOptions::new()
                    .with_resolver(&resolver)
                    .with_source_path(corpus["sourcePath"].as_str().unwrap())
                    .collecting_included(),
            );
        }
        let warnings = lint_carve_with_options(vector["entry"].as_str().unwrap(), &options);
        let mut actual: Vec<Value> = warnings.iter().map(|warning| {
            let mut finding = json!({ "rule": warning.rule, "file": warning.file.as_deref().unwrap_or(corpus["sourcePath"].as_str().unwrap()), "line": warning.line, "column": warning.column });
            if !warning.included_by.is_empty() {
                finding["includedBy"] = Value::Array(warning.included_by.iter().map(|site| json!({ "file": site.file, "line": site.line, "column": site.column })).collect());
            }
            finding
        }).collect();
        actual.sort_by_key(|item| {
            (
                item["file"].as_str().unwrap().to_string(),
                item["line"].as_u64().unwrap(),
                item["column"].as_u64().unwrap(),
                item["rule"].as_str().unwrap().to_string(),
            )
        });
        assert_eq!(
            Value::Array(actual),
            vector["expected"]["findings"],
            "{}",
            vector["name"]
        );
    }
}

#[test]
fn no_root_keeps_lint_output_unchanged() {
    let source = ">root\n\n{{ missing.crv }}\n";
    assert_eq!(
        lint_carve(source),
        lint_carve_with_options(source, &Options::new())
    );
}

#[test]
fn lint_normalizes_included_delimited_table_line_endings() {
    let cases = [
        ("crlf", "a\r\n\"x\"\r\n", None),
        ("cr_only_quoted", "\"x\"\r", None),
        ("crlf_quoted_lf", "a\r\n\"x\ny\"\r\n", None),
        (
            "cr_only_wide",
            "a,b\rc,d\re,f,g\r",
            Some(("delimited-table-wide-row", 3)),
        ),
        (
            "lf_wide",
            "a,b\nc,d\ne,f,g\n",
            Some(("delimited-table-wide-row", 3)),
        ),
    ];
    for (name, content, expected) in cases {
        let resolver = |path: &str, _: &IncludeContext<'_>| {
            (path == "p.csv")
                .then(|| IncludeResolved::from(content))
                .ok_or(IncludeDenial::Unresolved)
        };
        let options = Options::new().with_lint_includes(
            IncludeOptions::new()
                .with_resolver(&resolver)
                .with_source_path("doc.crv")
                .collecting_included(),
        );
        let warnings = lint_carve_with_options("{table}\n{{ p.csv @code:csv }}\n", &options);
        let actual: Vec<_> = warnings
            .iter()
            .map(|warning| (warning.rule, warning.line))
            .collect();
        assert_eq!(
            actual.as_slice(),
            expected.as_slice(),
            "{name}: {warnings:?}"
        );
    }
}

#[cfg(feature = "fs")]
#[test]
fn cli_reports_included_malformed_record() {
    use std::process::Command;
    let dir = std::env::temp_dir().join(format!("carve-lint-includes-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("data")).unwrap();
    std::fs::write(
        dir.join("doc.crv"),
        "{table}\n{{ data/plans.csv @code:csv }}\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("data/plans.csv"),
        "Plan,Seats,Price\nStarter,1,\"$0\"\nTeam,10,\"$49\" billed yearly\n",
    )
    .unwrap();
    let binary = env!("CARGO_BIN_EXE_carve");
    let with = Command::new(binary)
        .current_dir(&dir)
        .args(["lint", "--include-root", ".", "doc.crv"])
        .output()
        .unwrap();
    assert_eq!(
        with.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&with.stderr)
    );
    let text = String::from_utf8(with.stdout).unwrap();
    assert!(
        text.starts_with("data/plans.csv:3:1 delimited-table-malformed"),
        "{text}"
    );
    assert!(
        text.trim_end().ends_with("(included from doc.crv:2:1)"),
        "{text}"
    );
    let without = Command::new(binary)
        .current_dir(&dir)
        .args(["lint", "doc.crv"])
        .output()
        .unwrap();
    assert_eq!(without.status.code(), Some(0));
    assert!(without.stdout.is_empty());
    std::fs::remove_dir_all(dir).unwrap();
}
