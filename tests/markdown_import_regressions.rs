use carve::{
    markdown_to_carve, migrate_markdown, to_carve, try_markdown_to_carve, HtmlImportSeverity,
    MigrationConfidence, MigrationFidelity,
};

#[test]
fn empty_aligned_markdown_headers_have_a_source_form() {
    for source in [
        "|   |   |\n|:--|---|\n| 3 | 4 |\n",
        "|   |   |\n|---|:--|\n| 3 | 4 |\n",
        "|   |   |   |\n|---|:--|---|\n| 2 | 3 | 4 |\n",
    ] {
        let written = try_markdown_to_carve(source).unwrap();
        assert!(written.contains("|:---|"), "{written:?}");
        assert_eq!(to_carve(&written), written);
        assert!(carve::to_html(&written).contains("text-align: left"));
    }
}

#[test]
fn a_blank_body_row_is_kept_as_a_row() {
    let source = "Before\n\n| A | B |\n|---|---|\n|   |   |\n| 1 | 2 |\n\nAfter\n";
    let result = migrate_markdown(source);
    assert_eq!(
        result.value,
        "Before\n\n|= A |= B |\n| | |\n| 1 | 2 |\n\nAfter\n"
    );
    assert_eq!(try_markdown_to_carve(source).unwrap(), result.value);
    assert_eq!(to_carve(&result.value), result.value);
    assert!(!result
        .report
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "structure-unspellable"));
}

#[test]
fn an_empty_header_with_no_alignment_is_dropped_and_its_body_row_kept() {
    let source = "Before\n\n|   |   |\n|---|---|\n|   |   |\n\nAfter\n";
    let result = migrate_markdown(source);
    assert_eq!(result.value, "Before\n\n| | |\n\nAfter\n");
    let losses: Vec<_> = result
        .report
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "structure-unspellable")
        .collect();
    assert_eq!(losses.len(), 1);
    assert!(losses[0].message.contains("empty header row of 2 cells"));
    assert_eq!(losses[0].path.as_deref(), Some("line:3"));
    assert_eq!(losses[0].fidelity, MigrationFidelity::Dropped);
    assert_eq!(losses[0].confidence, MigrationConfidence::Exact);
    assert_eq!(losses[0].severity, HtmlImportSeverity::Warning);
}

#[test]
fn an_aligned_blank_row_keeps_its_alignment() {
    let source = "| A | B |\n|:-:|--:|\n|   |   |\n| 1 | 2 |\n";
    let result = migrate_markdown(source);
    assert_eq!(result.value, "|=~ A |=> B |\n|~ |> |\n|~ 1 |> 2 |\n");
    assert_eq!(to_carve(&result.value), result.value);
    assert!(carve::to_html(&result.value).contains(
        "<tr><td style=\"text-align: center;\"></td><td style=\"text-align: right;\"></td></tr>"
    ));

    let header = migrate_markdown("|   |   |\n|--:|---|\n| x | y |\n");
    assert_eq!(header.value, "|=> |= |\n|> x | y |\n");
    assert!(!header
        .report
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "structure-unspellable"));
}

/// The spec's importer-fidelity row for CARVE-P11-056, replayed as written.
#[test]
fn the_spec_blank_table_row_fixture_imports_as_recorded() {
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/spec/tests/importer-fidelity/manifest.json"
        ))
        .unwrap(),
    )
    .unwrap();
    let case = manifest["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["id"] == "markdown-blank-table-row")
        .expect("the pinned spec carries the fixture");
    let result = migrate_markdown(case["input"].as_str().unwrap());
    assert_eq!(result.value, case["expected"]["output"].as_str().unwrap());
    let rows: Vec<(String, Option<String>)> = result
        .report
        .diagnostics
        .iter()
        .map(|d| (d.code.to_string(), d.path.clone()))
        .collect();
    let expected: Vec<(String, Option<String>)> = case["expected"]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| {
            (
                d["code"].as_str().unwrap().to_string(),
                d["path"].as_str().map(str::to_string),
            )
        })
        .collect();
    assert_eq!(rows, expected);
}

#[test]
fn nul_characters_become_replacement_characters() {
    assert_eq!(markdown_to_carve("a\0b\0c\n"), "a\u{fffd}b\u{fffd}c\n");
}

#[test]
fn tabs_before_closing_heading_hashes_do_not_become_heading_text() {
    assert_eq!(markdown_to_carve("# a\t##\n"), "# a\n");
    assert_eq!(markdown_to_carve("# a\t##\t\n"), "# a\n");
    assert_eq!(markdown_to_carve("# a ##\t\n"), "# a\n");
    assert_eq!(markdown_to_carve("> # a\t##\n"), "> # a\n");
    assert_eq!(markdown_to_carve("# a\tb\n"), "# a\tb\n");
    assert_eq!(markdown_to_carve("a\t#\n---\n"), "## a\t#\n");
}

#[test]
fn task_markers_take_precedence_over_reference_links() {
    // markup-carve/carve#2273: cmark-gfm 0.29.0.gfm.13 is the reader the
    // importers answer to (markup-carve/carve#2187) and it reads a box for
    // every shape here, leaving the definition unused. An unused definition
    // renders nothing, so it may go out or stay.
    assert_eq!(
        markdown_to_carve("- [x] done\n\n[x]: /u \"Title\"\n"),
        "- [x] done\n\n[x]: /u \"Title\"\n"
    );
    assert_eq!(markdown_to_carve("- [x] done\n"), "- [x] done\n");
    assert_eq!(
        markdown_to_carve("- [x]\tdone\n\n[x]: /u\n"),
        "- [x] done\n\n[x]: /u\n"
    );
    // The ordered form keeps the marker as text (carve-rs#1886), which is the
    // same reading spelled where the writer has no box to put it.
    assert_eq!(
        markdown_to_carve("1. [x] done\n\n[x]: /u\n"),
        "1. [x] done\n\n[x]: /u\n"
    );
}

#[test]
fn ordered_task_markers_report_the_box_the_writer_cannot_spell() {
    let result = migrate_markdown("1. [x] done\n2. [ ] next\n");
    assert_eq!(result.value, "1. [x] done\n2. [ ] next\n");
    let codes: Vec<_> = result
        .report
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.fidelity == MigrationFidelity::Dropped)
        .map(|diagnostic| diagnostic.code.as_str())
        .collect();
    assert_eq!(codes, ["structure-unspellable", "structure-unspellable"]);
    for diagnostic in result
        .report
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.fidelity == MigrationFidelity::Dropped)
    {
        assert_eq!(
            diagnostic.message,
            "An ordered task item is not spellable as a Carve task item; the checkbox marker was kept as text"
        );
        assert_eq!(diagnostic.fidelity, MigrationFidelity::Dropped);
        assert_eq!(diagnostic.confidence, MigrationConfidence::Exact);
    }

    for source in [
        "- [x] done\n",
        "> 1. [x] done\n",
        "```\n1. [x] done\n```\n",
        "para\n2. [x] done\n",
        "1. [x]\n",
        "1.     [x] code\n",
        "1. a\n   - [x] b\n",
    ] {
        let codes: Vec<_> = migrate_markdown(source)
            .report
            .diagnostics
            .into_iter()
            .filter(|diagnostic| {
                diagnostic.code != "fidelity-unverified"
                    && diagnostic.fidelity == MigrationFidelity::Dropped
            })
            .map(|diagnostic| diagnostic.code)
            .collect();
        assert!(codes.is_empty(), "{source}");
    }

    for source in [
        "- a\n  1. [x] b\n",
        "para\n1. [x] done\n",
        "1. [x] \n",
        "1. [x]\t\n",
        "1. [X] done\n",
        "1.\t[x] done\n",
    ] {
        let codes: Vec<_> = migrate_markdown(source)
            .report
            .diagnostics
            .into_iter()
            .filter(|diagnostic| {
                diagnostic.code != "fidelity-unverified"
                    && diagnostic.fidelity == MigrationFidelity::Dropped
            })
            .map(|diagnostic| diagnostic.code)
            .collect();
        assert_eq!(codes, ["structure-unspellable"], "{source}");
    }
    let mixed = migrate_markdown("1. [x] done\n2. plain\n");
    assert_eq!(
        mixed
            .report
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "structure-unspellable")
            .count(),
        1
    );
}

#[test]
fn a_link_label_preserves_emphasis_matching_its_outer_span() {
    for (source, expected) in [
        ("*foo [*bar*](/url)*", "{/foo [{/bar/}](/url)/}\n"),
        ("**foo [**bar**](/url)**", "{*foo [{*bar*}](/url)*}\n"),
    ] {
        let result = migrate_markdown(source);
        assert_eq!(result.value, expected);
        assert!(!result
            .report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "structure-unspellable"));
        assert_eq!(to_carve(&result.value), result.value);
    }
}

#[test]
fn same_kind_nesting_in_one_label_still_reports_flattening() {
    for source in ["*a *b* c*", "*a [x _b *c*_](/u)*"] {
        let result = migrate_markdown(source);
        assert!(!result
            .report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "structure-unspellable"));
    }
}

#[test]
fn markdown_boundary_losses_name_the_source_line_without_changing_output() {
    for (source, line) in [
        ("``` f&ouml;&ouml;\nfoo\n```\n", 1),
        ("````;\n````\n", 1),
        ("before\n\n> ```föö\n> x\n> ```\n", 3),
        ("before\n\n- ```föö\n  x\n  ```\n", 3),
    ] {
        let result = migrate_markdown(source);
        assert_eq!(result.value, markdown_to_carve(source));
        let losses: Vec<_> = result
            .report
            .diagnostics
            .iter()
            .filter(|row| row.code == "structure-unspellable")
            .collect();
        assert_eq!(losses.len(), 1, "{source}");
        assert_eq!(losses[0].fidelity, MigrationFidelity::Dropped);
        assert_eq!(losses[0].confidence, MigrationConfidence::Exact);
        assert_eq!(
            losses[0].path.as_deref(),
            Some(format!("line:{line}").as_str()),
            "{source}"
        );
    }
}

#[test]
fn valid_markdown_constructs_do_not_report_boundary_losses() {
    for source in [
        "[link](url)",
        "![alt](image.png)",
        "`[link]()`",
        "\\[link]()",
        "```c++ metadata\nx\n```",
        "```&#99;\nx\n```",
        "```\nx\n```",
    ] {
        assert!(
            !migrate_markdown(source)
                .report
                .diagnostics
                .iter()
                .any(|row| row.code == "structure-unspellable"),
            "{source}"
        );
    }
}

#[test]
fn empty_destinations_after_typed_frontmatter_have_no_structure_loss() {
    for source in [
        "---toml\na = 1\nb = 2\n---\n\n[link]()\n",
        "---toml\r\na = 1\r\nb = 2\r\n---\r\n\r\n[link]()\r\n",
        "---toml\ra = 1\rb = 2\r---\r\r[link]()\r",
    ] {
        let result = migrate_markdown(source);
        assert!(!result
            .report
            .diagnostics
            .iter()
            .any(|row| row.code == "structure-unspellable"));
    }
}

#[test]
fn nested_image_content_does_not_claim_retained_link_titles() {
    for source in [
        "![a [b](<> \"t\") c](x.png)",
        "![a ![b](<> \"t\") c](x.png)",
    ] {
        let result = migrate_markdown(source);
        assert_eq!(result.value.trim(), "![a b c](x.png)");
        assert!(
            !result
                .report
                .diagnostics
                .iter()
                .any(|row| row.code == "structure-unspellable"),
            "{source}"
        );
    }
}

#[test]
fn empty_destination_inside_raw_html_wrappers_is_native() {
    assert!(!migrate_markdown("<span>[x](<> \"t\")</span>")
        .report
        .diagnostics
        .iter()
        .any(|row| row.code == "structure-unspellable"));
}

#[test]
fn explicit_markdown_destinations_keep_literal_brackets() {
    for (source, expected) in [
        ("[a](/x[y])", "[a](/x[y])\n"),
        ("![b](/p[q].png)", "![b](/p[q].png)\n"),
        ("[a](</x[y]>)", "[a](/x[y])\n"),
        ("[a](/x\\[y\\])", "[a](/x[y])\n"),
        ("[a][r]\n\n[r]: /x[y]", "[a][r]\n\n[r]: /x[y]\n"),
        ("[a](/x%5By%5D)", "[a](/x%5By%5D)\n"),
    ] {
        let result = migrate_markdown(source);
        assert_eq!(result.value, expected, "{source}");
        assert_eq!(to_carve(&result.value), result.value, "{source}");
        assert!(
            !result
                .report
                .diagnostics
                .iter()
                .any(|d| d.code == "fidelity-unverified"),
            "{source}"
        );
    }
}

#[test]
fn markdown_image_paragraphs_have_exact_assessment() {
    for (source, expected, codes) in [
        (
            "![x](/ü.png)",
            "![x](/ü.png)\n",
            vec!["markdown-paragraph", "markdown-image"],
        ),
        (
            "[a](/x[y]) and ![b](/p[q].png)",
            "[a](/x[y]) and ![b](/p[q].png)\n",
            vec!["markdown-paragraph", "markdown-link", "markdown-image"],
        ),
        (
            "![x](/u.png \"T\")",
            "![x](/u.png \"T\")\n",
            vec!["markdown-paragraph", "markdown-image"],
        ),
        (
            "a ![x](/u.png)",
            "a ![x](/u.png)\n",
            vec!["markdown-paragraph", "markdown-image"],
        ),
    ] {
        let result = migrate_markdown(source);
        assert_eq!(result.value, expected, "{source}");
        let found: Vec<_> = result
            .report
            .diagnostics
            .iter()
            .map(|d| d.code.as_str())
            .collect();
        assert_eq!(found, codes, "{source}");
        assert!(
            result
                .report
                .diagnostics
                .iter()
                .all(|d| d.path.as_deref() == Some("line:1")),
            "{source}"
        );
    }
    let reference = migrate_markdown("![x][r]\n\n[r]: /u.png");
    assert!(!reference
        .report
        .diagnostics
        .iter()
        .any(|d| d.code == "fidelity-unverified"));
    let two = migrate_markdown("![x](/a.png)\n![y](/b.png)");
    assert!(!two
        .report
        .diagnostics
        .iter()
        .any(|d| d.code == "fidelity-unverified"));
}

#[test]
fn image_alt_keeps_markdown_line_break() {
    let result = migrate_markdown("![a\nb](u)\n");
    assert_eq!(result.value, "![a\nb](u)\n");
    assert!(carve::to_html(&result.value).contains("alt=\"a\nb\""));
}

#[test]
fn note_shaped_text_inside_markdown_fence_stays_code() {
    let source = "x[^f]\n\n[^f]: n\n\n    ```\n    [^z]: z\n    ```\n";
    let converted = markdown_to_carve(source);
    assert_eq!(converted.matches("[^z]: z").count(), 1, "{converted}");
    let html = carve::to_html(&converted);
    assert!(html.contains("<code>[^z]: z"), "{html}");
    assert!(!html.contains("[^z]</p>"), "{html}");
}
