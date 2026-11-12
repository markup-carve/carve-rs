//! The Markdown construct assessment models a dropped destination as its
//! content and locates each `attribute-dropped` row at its source line
//! (markup-carve/carve#6013, the carve#6011 importer-fidelity row).

use carve::{markdown_to_carve, migrate_markdown, to_html};

fn report(markdown: &str) -> Vec<String> {
    migrate_markdown(markdown)
        .report
        .diagnostics
        .iter()
        .map(|row| {
            format!(
                "{} {} {} {} {}",
                row.code,
                row.severity.as_str(),
                row.fidelity.as_str(),
                row.confidence.as_str(),
                row.path.as_deref().unwrap_or("-"),
            )
        })
        .collect()
}

#[test]
fn the_ruled_row_reports_the_paragraph_and_each_dropped_destination_at_its_line() {
    let markdown = "Siehe [x](javascript:alert(1)) und ![b](data:image/png;base64,AA).\n";
    assert_eq!(migrate_markdown(markdown).value, "Siehe x und b.\n");
    assert_eq!(
        report(markdown),
        [
            "markdown-paragraph info preserved exact line:1",
            "attribute-dropped warning dropped exact line:1",
            "attribute-dropped warning dropped exact line:1",
        ]
    );
}

#[test]
fn entity_spelled_and_autolinked_schemes_are_located() {
    for markdown in [
        "Siehe [x](&#106;avascript:x) und.\n",
        "Siehe [x](java&#115;cript:x \"t\") und.\n",
        "Siehe <javascript:x> und.\n",
        "Siehe [x](<javascript:x>) und.\n",
    ] {
        assert_eq!(
            report(markdown),
            [
                "markdown-paragraph info preserved exact line:1",
                "attribute-dropped warning dropped exact line:1",
            ],
            "{markdown:?}"
        );
    }
}

#[test]
fn a_denied_definition_reports_at_its_own_line_and_covers_its_uses() {
    let markdown = "Siehe [x](&#106;avascript:x) und [y][r].\n\n[r]: javascript:y\n[s]: data:x\n";
    assert_eq!(migrate_markdown(markdown).value, "Siehe x und y.\n");
    assert_eq!(
        report(markdown),
        [
            "markdown-paragraph info preserved exact line:1",
            "attribute-dropped warning dropped exact line:1",
            "attribute-dropped warning dropped exact line:3",
            "attribute-dropped warning dropped exact line:4",
        ]
    );
}

#[test]
fn an_allowed_definition_in_a_footnote_body_keeps_its_link() {
    // carve-rs#4298
    for markdown in [
        "a[^1]\n\n[^1]: [x][r]\n\n    [r]: https://e\n",
        "a[^1]\n\n[^1]: [x][r]\n\n    [r]: <https://e>\n",
        "a[^1]\n\n[^1]: [x][r] [z][d]\n\n    [r]: https://e\n    [d]: javascript:x\n",
    ] {
        let carve = markdown_to_carve(markdown);
        assert!(carve.contains("[r]: https://e\n"), "{markdown:?}: {carve}");
        assert!(
            to_html(&carve).contains("<a href=\"https://e\">x</a>"),
            "{markdown:?}: {carve}"
        );
    }
}
