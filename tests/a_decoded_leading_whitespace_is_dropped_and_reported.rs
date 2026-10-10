//! Whitespace a decoded character reference puts at the head of a block does
//! not reach the document, and the report now names it (carve-rs#2446).
//!
//! The drop itself is markup-carve/carve#2595: the character goes rather than
//! being substituted, because `\ ` reads back as U+00A0 and a non-breaking
//! space is not the tab or space the author wrote.

use carve::{markdown_to_carve, migrate_markdown, MigrationConfidence, MigrationFidelity};

const MESSAGE: &str = "Dropped whitespace a decoded reference put at the start of a line; \
                       Carve spells no leading whitespace on a paragraph";

fn loss_rows(source: &str) -> Vec<carve::MigrationDiagnostic> {
    migrate_markdown(source)
        .report
        .diagnostics
        .into_iter()
        .filter(|row| row.message == MESSAGE)
        .collect()
}

#[test]
fn the_whitespace_is_dropped_from_every_block_head() {
    for (source, written) in [
        (
            "&#32;leading outside a table\n",
            "leading outside a table\n",
        ),
        ("&#9;tab leading\n", "tab leading\n"),
        ("&#32;&#32;&#32;&#32;four spaces\n", "four spaces\n"),
        // The writer carried it into these two, where the READER drops it
        // instead, so the written source did not read back as what was written.
        ("> &#32;in quote\n", "> in quote\n"),
        (
            "| &#32;x | y |\n| --- | --- |\n| a | b |\n",
            "|= x |= y |\n| a | b |\n",
        ),
    ] {
        assert_eq!(markdown_to_carve(source), written, "{source:?}");
    }
}

#[test]
fn every_drop_is_reported_once() {
    for source in [
        "&#32;leading outside a table\n",
        "&#9;tab leading\n",
        "&#32;&#32;&#32;&#32;four spaces\n",
        "> &#32;in quote\n",
        "| &#32;x | y |\n| --- | --- |\n| a | b |\n",
    ] {
        let rows = loss_rows(source);
        assert_eq!(rows.len(), 1, "{source:?}");
        assert_eq!(rows[0].code, "structure-unspellable", "{source:?}");
        assert_eq!(rows[0].fidelity, MigrationFidelity::Dropped, "{source:?}");
        assert_eq!(rows[0].confidence, MigrationConfidence::Exact, "{source:?}");
    }
}

/// THE CONTROL. `&nbsp;` decodes to U+00A0, which is neither a space nor a tab,
/// IS spellable at a line's head and survives - so no row is owed there.
#[test]
fn a_non_breaking_space_is_neither_dropped_nor_reported() {
    assert_eq!(
        markdown_to_carve("&nbsp;leading nbsp\n"),
        "\u{a0}leading nbsp\n"
    );
    assert!(loss_rows("&nbsp;leading nbsp\n").is_empty());
    assert!(migrate_markdown("&nbsp;leading nbsp\n")
        .report
        .diagnostics
        .iter()
        .all(|row| row.fidelity != MigrationFidelity::Dropped));
}

/// A reference INSIDE a line is untouched, and so is a line that CONTINUES a
/// paragraph: neither sits at the head of a block.
#[test]
fn whitespace_away_from_a_block_head_stays() {
    assert_eq!(markdown_to_carve("a&#32;b\n"), "a b\n");
    assert!(loss_rows("a&#32;b\n").is_empty());
    assert_eq!(markdown_to_carve("x\n&#32;second\n"), "x\n second\n");
    assert!(loss_rows("x\n&#32;second\n").is_empty());
}
