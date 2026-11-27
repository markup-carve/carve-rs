//! The remaining carve#6907 migration lint and the resolved label-comment rows.
use carve::lint::{LintOptions, lint};

fn alignment_findings(source: &str, upgrade: bool) -> Vec<(u32, u32, String)> {
    lint(
        source,
        &LintOptions {
            upgrade,
            ..LintOptions::default()
        },
    )
    .into_iter()
    .filter(|finding| finding.code == "deprecated-cell-alignment")
    .map(|finding| {
        let pos = finding.pos.expect("position");
        (pos.start_line, pos.start_column, finding.message)
    })
    .collect()
}

#[test]
fn empty_old_alignment_cells_report_under_upgrade() {
    for (source, column, prefix, braced) in [
        ("|?v |\n", 2, "?v", "{?v}"),
        ("|< |\n", 2, "<", "{<}"),
        ("|=~^ |\n", 3, "~^", "{~^}"),
        ("|?v{.c} |\n", 2, "?v{.c}", "{?v .c}"),
        ("| a | ?~ |\n", 7, "?~", "{?~}"),
    ] {
        let found = alignment_findings(source, true);
        assert_eq!(found.len(), 1, "{source:?}: {found:?}");
        assert_eq!((found[0].0, found[0].1), (1, column), "{source:?}");
        assert!(found[0].2.contains(&format!("prefix \"{prefix}\"")));
        assert!(found[0].2.contains(&format!("write \"{braced}\"")));
        assert!(alignment_findings(source, false).is_empty(), "{source:?}");
    }
}

#[test]
fn other_cells_keep_their_existing_boundary() {
    for source in [
        "|? |\n",
        "|^ |\n",
        "|{?v} |\n",
        "|\\?v |\n",
        "|?v|\n",
        "|?v text |\n",
        "|?v{.c}|\n",
    ] {
        let found = alignment_findings(source, true);
        assert_eq!(
            found.len(),
            usize::from(source == "|?v text |\n"),
            "{source:?}: {found:?}"
        );
    }
}

#[test]
fn u399_u400_u402_brackets_have_no_reference_label() {
    for source in ["[a %% b](/u) c\n", "^[a %% c] d\n", ":foo[a %% c] d\n"] {
        assert!(
            !lint(source, &LintOptions::default())
                .iter()
                .any(|finding| finding.code == "comment-in-reference-label"),
            "{source:?}"
        );
    }
}
