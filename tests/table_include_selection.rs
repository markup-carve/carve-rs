use carve::{
    expand_includes, lint_carve_with_options, parse_with_options, render_html, render_markdown,
    BlockNode, IncludeDenial, IncludeOptions, IncludeResolved, Options,
};
use serde_json::Value;
use std::time::Instant;

fn expand(source: &str, payload: &str) -> carve::IncludeResult {
    let resolver =
        |_: &str, _: &carve::IncludeContext<'_>| -> Result<IncludeResolved, IncludeDenial> {
            Ok(IncludeResolved::from(payload))
        };
    let doc = parse_with_options(source, &Options::default().with_positions(true));
    expand_includes(doc, source, &IncludeOptions::new().with_resolver(&resolver))
}

#[test]
fn physical_lines_on_marked_tables_suggest_record_selection() {
    let payload = "Name,Price\nA,1\nB,2\n";
    let resolver =
        |_: &str, _: &carve::IncludeContext<'_>| -> Result<IncludeResolved, IncludeDenial> {
            Ok(IncludeResolved::from(payload))
        };
    for lang in ["csv", "tsv"] {
        let source = format!("{{table}}\n{{{{ data.{lang} @code:{lang} @lines:2-3 }}}}");
        let options =
            Options::default().with_lint_includes(IncludeOptions::new().with_resolver(&resolver));
        let findings = lint_carve_with_options(&source, &options);
        assert!(
            findings
                .iter()
                .any(|f| f.rule == "include-lines-table-rows-hint" && f.line == 2),
            "{lang}: {findings:?}"
        );
    }
}

#[test]
fn nested_table_line_hint_uses_the_directive_file() {
    let resolver =
        |path: &str, _: &carve::IncludeContext<'_>| -> Result<IncludeResolved, IncludeDenial> {
            match path {
                "child.crv" => Ok(IncludeResolved::from(
                    "{table}\n{{ data.csv @code:csv @lines:2-3 }}",
                )),
                "data.csv" => Ok(IncludeResolved::from("Name,Price\nA,1\nB,2\n")),
                _ => Err(IncludeDenial::NotFound),
            }
        };
    let options = Options::default().with_lint_includes(
        IncludeOptions::new()
            .with_resolver(&resolver)
            .with_source_path("root.crv"),
    );
    let findings = lint_carve_with_options("{{ child.crv }}", &options);
    assert!(
        findings
            .iter()
            .any(|f| f.rule == "include-lines-table-rows-hint"
                && f.file.as_deref() == Some("child.crv")
                && f.line == 2),
        "{findings:?}"
    );
}

#[test]
fn physical_line_slices_inside_quoted_records_fall_back_to_code() {
    let file = "A,B\n\"first\nmiddle\nlast\",x\nZ,y\n";
    for line in [2, 3, 4] {
        let source = format!("{{table}}\n{{{{ data.csv @code:csv @lines:{line}-{line} }}}}");
        let result = expand(&source, file);
        let html = render_html(&result.doc).unwrap();
        assert!(html.contains("<pre"), "line {line}: {html}");
        assert!(!html.contains("<table"), "line {line}: {html}");
    }
}

#[test]
fn selected_cells_keep_included_file_positions() {
    let source = "{table}\n{{ data.csv @code:csv @rows:3-3 @cols:\"Price,Name\" }}";
    let result = expand(source, "Name,Price\nA,1\nB,2\n");
    let BlockNode::Table(table) = &result.doc.children[0] else {
        panic!("expected table")
    };
    let header = table.rows[0].cells[0].pos.as_ref().unwrap();
    let body = table.rows[1].cells[0].pos.as_ref().unwrap();
    assert_eq!(
        (header.start_line, header.start_column, header.start_offset),
        (1, 6, 5)
    );
    assert_eq!(
        (body.start_line, body.start_column, body.start_offset),
        (3, 3, 17)
    );
    assert_eq!(header.file.as_ref().unwrap().as_str(), "data.csv");
    assert_eq!(body.file.as_ref().unwrap().as_str(), "data.csv");
}

#[test]
fn invalid_table_options_keep_the_dependency_and_expected_rule() {
    let file = "Name,Price\nA,1\n";
    for (option, rule) in [
        ("@rows:0-2", "include-rows-invalid"),
        ("@rows:9-10", "include-rows-out-of-range"),
        ("@cols:\"Missing\"", "include-cols-invalid"),
        ("@cols:\"Name,Name\"", "include-cols-duplicate"),
        ("@cols:\"Name,\"", "include-cols-invalid"),
        ("@cols:1-2", "include-cols-invalid"),
        ("@rows:2-2 @lines:2-2", "include-selection-conflict"),
        ("@rows:2-2 @rows:2-2", "include-duplicate-option"),
    ] {
        let source = format!("{{table}}\n{{{{ data.csv @code:csv {option} }}}}");
        let result = expand(&source, file);
        assert!(
            result.warnings.iter().any(|w| w.rule == rule),
            "{option}: {:?}",
            result.warnings
        );
        assert_eq!(result.dependencies.len(), 1, "{option}");
        assert!(result.dependencies[0].resolved, "{option}");
        assert!(
            !render_html(&result.doc).unwrap().contains("<table"),
            "{option}"
        );
    }
}

#[test]
#[ignore = "manual include timing measurement"]
fn include_selection_timing() {
    for count in [2_000, 8_000, 32_000] {
        let mut payload = String::from("Name,Price\n");
        for index in 1..=count {
            payload.push_str(&format!("row{index},{index}\n"));
        }
        let source = format!(
            "{{table}}\n{{{{ data.csv @code:csv @rows:2-{} @cols:\"Price,Name\" }}}}",
            count + 1
        );
        let mut times = Vec::new();
        for _ in 0..3 {
            let start = Instant::now();
            let result = expand(&source, &payload);
            assert!(result.warnings.is_empty());
            times.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        times.sort_by(f64::total_cmp);
        println!("selection {count} rows: {:.1} ms median", times[1]);
    }
}

#[test]
fn pinned_table_selection_corpus() {
    let corpus: Value =
        serde_json::from_str(include_str!("spec/tests/include-selection/cases.json")).unwrap();
    for case in corpus["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let lang = case["lang"].as_str().unwrap_or("csv");
        let options = case["options"].as_str().unwrap();
        let (section, rest) = if options.starts_with('#') {
            options.split_once(' ').unwrap_or((options, ""))
        } else {
            ("", options)
        };
        let directive = format!("{{{{ data.{lang}{section} @code:{lang} {rest} }}}}");
        let source = format!(
            "{}{}{}",
            case["attrs"]
                .as_str()
                .map_or(String::new(), |s| format!("{s}\n")),
            directive,
            case["caption"]
                .as_str()
                .map_or(String::new(), |s| format!("\n^ {s}"))
        );
        let result = expand(&source, case["file"].as_str().unwrap());
        let html = render_html(&result.doc).unwrap();
        if case["literal"].as_bool() == Some(true) {
            assert!(!html.contains("<table"), "{name}: {html}");
            assert!(!result.warnings.is_empty(), "{name}");
            assert_eq!(result.dependencies.len(), 1, "{name}");
            assert!(result.dependencies[0].resolved, "{name}");
        } else if case["codeFallback"].as_bool() != Some(true) {
            assert!(html.contains("<table"), "{name}: {html}");
        }
        for value in case["html"].as_array().into_iter().flatten() {
            assert!(html.contains(value.as_str().unwrap()), "{name}: {html}");
        }
        for value in case["absent"].as_array().into_iter().flatten() {
            assert!(!html.contains(value.as_str().unwrap()), "{name}: {html}");
        }
        if let Some(markdown) = case["markdown"].as_str() {
            assert_eq!(
                render_markdown(&result.doc).unwrap().trim(),
                markdown,
                "{name}"
            );
        }
    }
}

#[test]
fn region_marker_lines_are_no_table_records() {
    let file = "%% #region r\nName,Price\nA,1\n%% #endregion r\nB,2\n";
    let nested = "Name\n%% #region o\nA\n// #region i\nB\n/* #endregion i */\n%% #endregion o\nC\n";
    let kept = "%% #region\nName\n\"x\n%% #region q\"\n<!-- #region h\n";
    for (options, payload, present, absent) in [
        (
            "",
            file,
            &["<th scope=\"col\">Name</th>", "<td>A</td>", "<td>B</td>"][..],
            &["region"][..],
        ),
        (
            "@rows:2-2",
            file,
            &["<th scope=\"col\">Name</th>", "<td>A</td>"],
            &["region", "<td>B</td>"],
        ),
        (
            "@lines:1-3",
            file,
            &["<th scope=\"col\">Name</th>", "<td>A</td>"],
            &["region", "<td>B</td>"],
        ),
        (
            "@region:o",
            nested,
            &["<td>A</td>", "<td>B</td>"],
            &["region", "<td>C</td>"],
        ),
        (
            "",
            kept,
            &[
                "<th scope=\"col\">%% #region</th>",
                "<td>x\n%% #region q</td>",
                "<td>&lt;!-- #region h</td>",
            ],
            &[],
        ),
    ] {
        let source = format!("{{table}}\n{{{{ data.csv @code:csv {options} }}}}");
        let html = render_html(&expand(&source, payload).doc).unwrap();
        for value in present {
            assert!(
                html.contains(value),
                "{options} {payload:?}: {value}\n{html}"
            );
        }
        for value in absent {
            assert!(
                !html.contains(value),
                "{options} {payload:?}: absent {value}\n{html}"
            );
        }
    }
    let fence = "{table}\n```csv\n%% #region r\nA,1\n```\n";
    let doc = parse_with_options(fence, &Options::default());
    let html = render_html(&doc).unwrap();
    assert!(
        html.contains("<th scope=\"col\">%% #region r</th>"),
        "{html}"
    );
}

#[test]
fn lint_reads_table_include_records_without_region_markers() {
    for (payload, options) in [
        ("%% #region r\nName,Price\nA,1\n%% #endregion r\n", ""),
        (
            "%% #region r\r\nName,Price\r\nA,1\r\n%% #endregion r\r\n",
            "@rows:2-2",
        ),
    ] {
        let resolver =
            |_: &str, _: &carve::IncludeContext<'_>| -> Result<IncludeResolved, IncludeDenial> {
                Ok(IncludeResolved::from(payload))
            };
        let lint =
            Options::default().with_lint_includes(IncludeOptions::new().with_resolver(&resolver));
        let source = format!("{{table}}\n{{{{ data.csv @code:csv {options} }}}}");
        let findings = lint_carve_with_options(&source, &lint);
        assert!(
            !findings
                .iter()
                .any(|f| f.rule == "delimited-table-wide-row"),
            "{payload:?}: {findings:?}"
        );
    }
}

#[test]
fn generated_record_and_column_projection() {
    for variant in 0..1000 {
        let tsv = variant % 2 == 1;
        let separator = if tsv { "\t" } else { "," };
        let format = if tsv { "tsv" } else { "csv" };
        let headerless = variant % 3 == 0;
        let first = 1 + variant % 5;
        let last = (first + variant % 3).min(6);
        let left = variant % 3;
        let right = (left + 1) % 4;
        let headers = ["A", "B", "C", "D"];
        let mut data = Vec::new();
        data.push(headers.join(separator));
        for row in 1..=6 {
            data.push(
                (1..=4)
                    .map(|col| format!("r{row}c{col}"))
                    .collect::<Vec<_>>()
                    .join(separator),
            );
        }
        let payload = format!("{}\n", data.join("\n"));
        let attrs = if headerless {
            "{table header-rows=0}"
        } else {
            "{table}"
        };
        let cols = if headerless {
            format!("{}-{}", left + 1, left + 2)
        } else {
            format!("\"{},{}\"", headers[left], headers[right])
        };
        let source = format!(
            "{attrs}\n{{{{ data.{format} @code:{format} @rows:{first}-{last} @cols:{cols} }}}}"
        );
        let result = expand(&source, &payload);
        assert!(
            result.warnings.is_empty(),
            "variant {variant}: {:?}",
            result.warnings
        );
        let BlockNode::Table(table) = &result.doc.children[0] else {
            panic!("variant {variant}: expected table")
        };
        let expected_count = if headerless {
            last - first + 1
        } else {
            1 + (last + 1).saturating_sub(first.max(2))
        };
        assert_eq!(table.rows.len(), expected_count, "variant {variant}");
        assert_eq!(table.rows[0].cells.len(), 2, "variant {variant}");
        let html = render_html(&result.doc).unwrap();
        assert!(html.contains("<table"), "variant {variant}: {html}");
    }
}

#[test]
fn generated_container_and_multiline_variants() {
    for variant in 0..1000 {
        let tsv = variant % 2 == 1;
        let separator = if tsv { "\t" } else { "," };
        let lang = if tsv { "tsv" } else { "csv" };
        let selected = 2 + variant % 4;
        let mut records = vec![format!("Name{separator}Value{separator}Other")];
        for row in 1..=6 {
            let name = if row == 1 && variant % 5 == 0 {
                "\"multi\nline\"".to_string()
            } else {
                format!("row{row}")
            };
            records.push(format!("{name}{separator}v{row}{separator}x{row}"));
        }
        let payload = format!("{}\n", records.join("\n"));
        let entry = format!("{{table}}\n{{{{ data.{lang} @code:{lang} @rows:{selected}-{selected} @cols:\"Value,Name\" }}}}");
        let source = match variant % 5 {
            0 => format!("{entry}\n\nAfter."),
            1 => format!("> {}\n> After.", entry.replace('\n', "\n> ")),
            2 => format!("> > {}\n> > After.", entry.replace('\n', "\n> > ")),
            3 => format!("- {}\n\nAfter.", entry.replace('\n', "\n  ")),
            _ => format!("::: note\n{entry}\nAfter.\n:::"),
        };
        let result = expand(&source, &payload);
        assert!(
            result.warnings.is_empty(),
            "variant {variant}: {:?}",
            result.warnings
        );
        let html = render_html(&result.doc).unwrap();
        assert!(html.contains("<table"), "variant {variant}: {html}");
        let expected_name = if selected == 2 && variant % 5 == 0 {
            "multi\nline".to_string()
        } else {
            format!("row{}", selected - 1)
        };
        assert!(
            html.contains(&format!(
                "<td>v{}</td><td>{expected_name}</td>",
                selected - 1
            )),
            "variant {variant}: {html}"
        );
        assert!(html.contains("After."), "variant {variant}: {html}");
    }
}

#[test]
fn a_cols_backslash_escapes_ascii_punctuation_only() {
    // carve#4571: `\z` is a backslash and a `z`, so no header is named `a\zb`.
    let result = expand(
        "{table}\n{{ z.csv @code:csv @cols:\"a\\zb\" }}",
        "azb,c\n1,2\n",
    );
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.rule == "include-cols-invalid"),
        "{:?}",
        result.warnings
    );
    assert!(!render_html(&result.doc).unwrap().contains("<table"));
    let result = expand(
        "{table}\n{{ z.csv @code:csv @cols:\"a\\,b\" }}",
        "\"a,b\",c\n1,2\n",
    );
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    assert!(render_html(&result.doc)
        .unwrap()
        .contains("<th scope=\"col\">a,b</th>"));
}

#[test]
fn a_quoted_option_value_selects_like_its_bare_spelling() {
    let payload = "Name,Qty\nA,1\nB,2\n";
    let bare = render_html(&expand("{{ data.csv @code:csv @lines:2-2 }}", payload).doc).unwrap();
    for quoted in [
        r#"@lines:"2-2""#,
        "@lines:'2-2'",
        r#"@code:"csv" @lines:2-2"#,
    ] {
        let source =
            format!("{{{{ data.csv @code:csv {quoted} }}}}").replace("@code:csv @code", "@code");
        let result = expand(&source, payload);
        assert!(
            result.warnings.is_empty(),
            "{quoted}: {:?}",
            result.warnings
        );
        assert_eq!(render_html(&result.doc).unwrap(), bare, "{quoted}");
    }
}
