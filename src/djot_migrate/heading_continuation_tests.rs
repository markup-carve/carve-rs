use super::djot_to_carve;

#[test]
fn headings_fold_matching_markers_and_lazy_lines() {
    for (source, expected) in [
        ("# Heading\n# continued\n", "# Heading continued\n"),
        ("# Heading\nlazy\n", "# Heading lazy\n"),
        (
            "# Heading\nlazy\n# more\nlazy\n\ntext\n",
            "# Heading lazy more lazy\n\ntext\n",
        ),
        ("## A\n## B\nC", "## A B C"),
        ("# A\n  # B\n  C", "# A B C"),
        ("# A\n    B", "# A B"),
    ] {
        assert_eq!(djot_to_carve(source), expected);
    }
}

/// A percent run is not a block starter in Djot, so it folds onto the
/// heading like any other lazy continuation line (carve-rs#2453). The
/// two-sign run always folded; the three-sign one did not, because the
/// folder's block-starter list carried the Carve comment fence, and
/// Carve's two-sign line comment is not in the same list.
#[test]
fn a_percent_run_folds_onto_a_heading_at_every_run_length() {
    for (djot, carve, html) in [
        (
            "# A\n%\n",
            "# A %\n",
            "<section id=\"A\">\n  <h1>A %</h1>\n</section>",
        ),
        (
            "# A\n%%\n",
            "# A \\%%\n",
            "<section id=\"A\">\n  <h1>A %%</h1>\n</section>",
        ),
        (
            "# A\n%%%\n",
            "# A \\%%%\n",
            "<section id=\"A\">\n  <h1>A %%%</h1>\n</section>",
        ),
        (
            "# A\n%%%%\n",
            "# A \\%%%%\n",
            "<section id=\"A\">\n  <h1>A %%%%</h1>\n</section>",
        ),
        (
            "## A\n%%%\n",
            "## A \\%%%\n",
            "<section id=\"A\">\n  <h2>A %%%</h2>\n</section>",
        ),
        (
            "###### A\n%%%\n",
            "###### A \\%%%\n",
            "<section id=\"A\">\n  <h6>A %%%</h6>\n</section>",
        ),
        (
            "# A\nB\n",
            "# A B\n",
            "<section id=\"A-B\">\n  <h1>A B</h1>\n</section>",
        ),
        ("A\n%%%\n", "A\n\\%%%\n", "<p>A\n%%%</p>"),
        ("%%%\nX\n", "\\%%%\nX\n", "<p>%%%\nX</p>"),
    ] {
        assert_eq!(djot_to_carve(djot), carve);
        assert_eq!(crate::to_html(carve), html);
    }
}

#[test]
fn headings_stop_at_blocks_and_leave_code_alone() {
    for next in [
        "",
        "## B",
        "#",
        "- item",
        "1. item",
        "> quote",
        "```",
        "~~~",
        "[r]: /url",
        "::: div",
        "  - item",
        "***",
        "---",
        "* * *",
        "^ x",
    ] {
        let source = format!("# A\n{next}\n");
        assert_eq!(
            djot_to_carve(&source),
            format!("# A\n{}", djot_to_carve(&format!("{next}\n")))
        );
    }
    assert_eq!(djot_to_carve("# A\n{.class}\n"), "# A\n");
    assert_eq!(djot_to_carve("para\n# A\nB\n"), "para\n\\# A\nB\n");
    for source in ["```\n# A\n# B\n```\n", "`x\n# A\n# B\ny`\n", "# A\\\nB\n"] {
        assert_eq!(djot_to_carve(source), source);
    }
}
