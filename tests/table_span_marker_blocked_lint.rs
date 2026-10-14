use carve::lint_carve;

fn findings(source: &str) -> Vec<carve::LintWarning> {
    lint_carve(source)
        .into_iter()
        .filter(|warning| warning.rule == "table-span-marker-blocked")
        .collect()
}

#[test]
fn blocked_markers_report_the_marker_and_reason() {
    let cases = [
        ("| ^ | b |\n", 1, 3, "There is no cell above it to extend."),
        ("| a | b |\n| < | c |\n", 2, 3, "There is no cell to its left to widen."),
        ("| a | < |\n| ^ | b |\n", 2, 3, "Extending the cell above would cover another cell."),
        ("| a | b | c |\n| ^ | < | d |\n", 2, 7, "The cell to its left starts in an earlier row, so widening it would cover cells already placed."),
        ("| a | b |\n| < | c |\n+ x | y |\n", 2, 3, "There is no cell to its left to widen."),
        ("- item\n\n  | ^ | b |\n", 3, 5, "There is no cell above it to extend."),
        ("x[^n]\n\n[^n]: note\n\n    | ^ | b |\n", 5, 7, "There is no cell above it to extend."),
        ("| ^ | b |\n^ Table: cap\n", 1, 3, "There is no cell above it to extend."),
        ("| a |\n| b | ^ |\n", 2, 7, "There is no cell above it to extend."),
        ("| `a|b` | ^ |\n+ x | y |\n", 1, 11, "There is no cell above it to extend."),
    ];
    for (source, line, column, reason) in cases {
        let warnings = findings(source);
        assert_eq!(warnings.len(), 1, "{source:?}: {warnings:?}");
        let warning = &warnings[0];
        let start = source
            .split('\n')
            .take(line - 1)
            .map(|line| line.len() + 1)
            .sum::<usize>()
            + column
            - 1;
        let marker = &source[start..start + 1];
        assert!(matches!(marker, "^" | "<"));
        assert_eq!(
            (warning.line, warning.column, warning.start, warning.end),
            (line, column, start, start + 1),
            "{source:?}"
        );
        assert_eq!(warning.message, format!("The \"{marker}\" span marker cannot extend a cell. {reason} It renders as a cell of its own; escape it as \"\\{marker}\" to keep the character as text, or remove it."));
    }
}

#[test]
fn resolved_and_literal_markers_are_not_reported() {
    for source in [
        "| a | < |\n| ^ | ^ |\n",
        "| a | b |\n| ^ | c |\n",
        "| \\^ | b |\n",
        "| \\< | b |\n",
        "| a | b |\n|---|---|\n| ^ | c |\n",
        "```\n| ^ | b |\n```\n",
    ] {
        assert!(findings(source).is_empty(), "{source:?}");
    }
}

#[test]
fn byte_offsets_follow_an_astral_character() {
    let source = "😀\n> | ^ | b |\n";
    let warnings = findings(source);
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    let warning = &warnings[0];
    let start = source.find('^').unwrap();
    assert_eq!(
        (warning.line, warning.column, warning.start, warning.end),
        (2, 5, start, start + 1)
    );
}

#[test]
fn continuation_fallback_uses_byte_offsets_with_crlf() {
    let source = "| a | b |\r\n| < | c |\r\n+ x | y |\r\n";
    let warning = findings(source).pop().expect("blocked marker");
    let start = source.find('<').unwrap();
    assert_eq!(
        (warning.line, warning.column, warning.start, warning.end),
        (2, 3, start, start + 1)
    );
}
