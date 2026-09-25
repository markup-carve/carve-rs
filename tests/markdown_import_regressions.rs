use carve::{
    markdown_to_carve, migrate_markdown, to_carve, try_markdown_to_carve, HtmlImportSeverity,
    MigrationConfidence, MigrationFidelity,
};

#[test]
fn blank_table_rows_are_reported_without_erasing_surrounding_content() {
    let source = "Before\n\n| A | B |\n|---|---|\n|   |   |\n| 1 | 2 |\n\nAfter\n";
    let result = migrate_markdown(source);
    assert_eq!(result.value, "Before\n\n|= A |= B |\n| 1 | 2 |\n\nAfter\n");
    assert_eq!(try_markdown_to_carve(source).unwrap(), result.value);
    assert_eq!(to_carve(&result.value), result.value);

    let losses: Vec<_> = result
        .report
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "structure-unspellable")
        .collect();
    assert_eq!(losses.len(), 1);
    assert_eq!(losses[0].fidelity, MigrationFidelity::Dropped);
    assert_eq!(losses[0].confidence, MigrationConfidence::Exact);
    assert_eq!(losses[0].severity, HtmlImportSeverity::Warning);
}

#[test]
fn wholly_blank_table_is_removed_with_one_warning_per_row() {
    let source = "Before\n\n|   |   |\n|---|---|\n|   |   |\n\nAfter\n";
    let result = migrate_markdown(source);
    assert_eq!(result.value, "Before\n\nAfter\n");
    assert_eq!(
        result
            .report
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "structure-unspellable")
            .count(),
        2
    );
    assert!(result.report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "structure-unspellable"
            && diagnostic
                .message
                .contains("header table row of 2 blank cells")
    }));
}

#[test]
fn aligned_blank_table_row_is_dropped_without_corrupting_cells() {
    let source = "| A | B |\n|:--|--:|\n|   |   |\n| 1 | 2 |\n";
    let result = migrate_markdown(source);
    assert_eq!(result.value, "|=< A |=> B |\n|< 1 |> 2 |\n");
    assert_eq!(to_carve(&result.value), result.value);
    assert_eq!(
        result
            .report
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "structure-unspellable")
            .count(),
        1
    );
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
fn reference_links_take_precedence_over_task_markers() {
    assert_eq!(
        markdown_to_carve("1. [x] done\n\n[x]: /u\n"),
        "1. [x](/u) done\n"
    );
    assert_eq!(
        markdown_to_carve("- [x] done\n\n[x]: /u \"Title\"\n"),
        "- [x](/u \"Title\") done\n"
    );
    assert_eq!(markdown_to_carve("- [x] done\n"), "- [x] done\n");
    assert_eq!(
        markdown_to_carve("- [x]\tdone\n\n[x]: /u\n"),
        "- [x](/u)\tdone\n"
    );
}
