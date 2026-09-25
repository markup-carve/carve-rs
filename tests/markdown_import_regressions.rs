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
fn task_markers_take_precedence_over_reference_links() {
    // markup-carve/carve#2273: cmark-gfm 0.29.0.gfm.13 is the reader the
    // importers answer to (markup-carve/carve#2187) and it reads a box for
    // every shape here, leaving the definition unused. An unused definition
    // renders nothing, so it may go out or stay.
    assert_eq!(
        markdown_to_carve("- [x] done\n\n[x]: /u \"Title\"\n"),
        "- [x] done\n"
    );
    assert_eq!(markdown_to_carve("- [x] done\n"), "- [x] done\n");
    assert_eq!(
        markdown_to_carve("- [x]\tdone\n\n[x]: /u\n"),
        "- [x] done\n"
    );
    // The ordered form keeps the marker as text (carve-rs#1886), which is the
    // same reading spelled where the writer has no box to put it.
    assert_eq!(
        markdown_to_carve("1. [x] done\n\n[x]: /u\n"),
        "1. [x] done\n"
    );
}
