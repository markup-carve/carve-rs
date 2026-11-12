//! The Markdown construct assessment models a dropped destination as exactly
//! what the import writes: no block for an emptied paragraph or formatting
//! run, plain alt text for an image, and no edge spaces (markup-carve/carve#6014).

use carve::migrate_markdown;

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
fn an_emptied_paragraph_reports_only_the_dropped_destination() {
    for markdown in ["![](data:x)\n", "[](javascript:x)\n", "*![](data:x)*\n"] {
        assert_eq!(migrate_markdown(markdown).value, "\n", "{markdown:?}");
        assert_eq!(
            report(markdown),
            ["attribute-dropped warning dropped exact line:1"],
            "{markdown:?}"
        );
    }
}

#[test]
fn formatted_alt_text_is_modeled_as_the_plain_text_the_import_writes() {
    for (markdown, value) in [
        ("x ![a *b*](data:x) y\n", "x a b y\n"),
        ("x ![a `c` **d**](data:x) y\n", "x a c d y\n"),
        ("x ![a [b](https://e) c](data:x) y\n", "x a b c y\n"),
        ("x ![a ~~b~~ c](javascript:x)\n", "x a b c\n"),
    ] {
        assert_eq!(migrate_markdown(markdown).value, value, "{markdown:?}");
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
fn the_spaces_a_drop_leaves_at_a_run_edge_are_modeled_trimmed() {
    for (markdown, value, first) in [
        (
            "*![](data:x)* y\n",
            "y\n",
            "markdown-paragraph info preserved exact line:1",
        ),
        (
            "x ![](data:x)\n",
            "x\n",
            "markdown-paragraph info preserved exact line:1",
        ),
        (
            "[ t ](javascript:x)\n",
            "t\n",
            "markdown-paragraph info preserved exact line:1",
        ),
        (
            "# ![](data:x) a\n",
            "# a\n",
            "markdown-atx-heading info preserved exact line:1",
        ),
    ] {
        assert_eq!(migrate_markdown(markdown).value, value, "{markdown:?}");
        assert_eq!(
            report(markdown),
            [first, "attribute-dropped warning dropped exact line:1"],
            "{markdown:?}"
        );
    }
}

#[test]
fn a_tight_item_and_a_line_break_left_at_the_edge() {
    assert_eq!(migrate_markdown("- ![](data:x) y\n").value, "- y\n");
    assert_eq!(
        report("- ![](data:x) y\n"),
        [
            "markdown-bullet-list info preserved exact line:1",
            "markdown-list-item info preserved exact line:1",
            "attribute-dropped warning dropped exact line:1",
        ]
    );
    assert_eq!(migrate_markdown("x\n![](data:x)\n").value, "x\n");
    assert_eq!(
        report("x\n![](data:x)\n"),
        [
            "markdown-paragraph info preserved exact line:1",
            "attribute-dropped warning dropped exact line:2",
        ]
    );
}
