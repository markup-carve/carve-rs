use carve::lint_carve;

#[test]
fn quoted_definition_shaped_paragraphs_are_reported() {
    for source in [
        "> ::\tterm\n> : d\n",
        "> > ::\tterm\n> > : d\n",
        "> - ::\tterm\n>   : d\n",
    ] {
        let warnings: Vec<_> = lint_carve(source)
            .into_iter()
            .filter(|warning| warning.rule == "markdown-definition-paragraph")
            .collect();
        assert_eq!(warnings.len(), 1, "{source:?}: {warnings:?}");
        assert_eq!(warnings[0].line, 2, "{source:?}");
        assert_eq!(&source[warnings[0].start..warnings[0].end], ":");
    }
}

fn findings(source: &str, rule: &str) -> Vec<(usize, usize)> {
    lint_carve(source)
        .into_iter()
        .filter(|finding| finding.rule == rule)
        .map(|finding| (finding.line, finding.column))
        .collect()
}

#[test]
fn heading_jump_honors_same_line_directive() {
    assert!(findings(
        "# One\n\n### Three %% carve-lint-disable-line heading-level-skip\n",
        "heading-level-skip"
    )
    .is_empty());
    assert_eq!(
        findings(
            "# One %% carve-lint-disable-line heading-level-skip\n\n### Three\n",
            "heading-level-skip"
        ),
        vec![(3, 1)]
    );
}

#[test]
fn table_paragraphs_with_open_pipes_are_reported() {
    assert_eq!(
        findings("|a|b|\x0c\n", "table-shaped-paragraph"),
        vec![(1, 1)]
    );
    assert_eq!(
        findings(
            "- text\n\n  | a |\n  |b\n\n  ^ cap\n- c\n",
            "table-shaped-paragraph"
        ),
        vec![(4, 3)]
    );
}

#[test]
fn invalid_separator_padding_is_a_near_miss() {
    for source in [
        "| a | b |\n|\t--- |\t--- |\n| 1 | 2 |\n",
        "| a | b |\n| \t--- | \t--- |\n| 1 | 2 |\n",
        "| a | b |\n| ---\t| ---\t|\n| 1 | 2 |\n",
    ] {
        assert_eq!(findings(source, "table-separator-near-miss"), vec![(2, 1)]);
    }
}

#[test]
fn a_marker_folded_into_a_note_paragraph_is_reported() {
    // A marker in a note body opens a list there, so no paragraph holds it
    // (carve#4860).
    for source in [
        ":: t\n:  [^f]: note\n     - nested\n   tail\n",
        ". d\n   [^n]: e\n     - a\n",
        "- > para\n  > [^f]: n\n  >   - sub\ntail[^f]\n",
        "[^n]: d\n  - para\n    [r]: /u\nx\n\n[r][] [^n]\n",
    ] {
        assert!(
            findings(source, "list-marker-without-blank-line").is_empty(),
            "{source}"
        );
    }
}

#[test]
fn a_list_separated_in_a_note_is_quiet() {
    assert!(findings(
        "[^n]: d\n\n  - a\n\nr[^n]\n",
        "list-marker-without-blank-line"
    )
    .is_empty());
}

#[test]
fn invisible_lines_release_folded_marker_candidates() {
    for source in [
        "::: note\n- para\n- %% c\n```\ncode\n::::\n:::\nafter *b*\n",
        "::: note\n- para\n- {#x}\n```\ncode\n::::\n:::\nafter *b*\n",
    ] {
        assert!(findings(source, "list-marker-without-blank-line").is_empty());
    }
}

#[test]
fn parsed_empty_links_are_reported() {
    assert_eq!(findings("[x]()\n", "empty-link-destination"), vec![(1, 1)]);
}

#[test]
fn unused_winning_references_are_reported() {
    assert_eq!(
        findings(
            "Text.\n\n[x]: /u\n\n[y]: javascript:alert(3)\n",
            "unused-link-reference-definition"
        ),
        vec![(3, 1), (5, 1)]
    );
}

#[test]
fn link_destination_bytes_are_not_prose_for_source_lint() {
    assert!(findings("[pre <http://h> post](/u)\n", "unclosed-inline-emphasis").is_empty());
    assert!(findings("[x](a`b{c}d|e^f<g>h)\n", "bare-html-tag").is_empty());
}

#[test]
fn generated_context_variants() {
    fn wrap(source: String, n: usize) -> String {
        match n % 6 {
            0 => source,
            1 => format!("intro{n}\n\n{source}"),
            2 => format!("::: note\n{source}:::\n"),
            3 => format!("::: >\n{source}:::\n"),
            4 => format!("::: note\n::: >\n{source}:::\n:::\n"),
            _ => format!("::: note\n::: note\n{source}:::\n:::\n"),
        }
    }
    for n in 0..1000 {
        let word = format!("w{n}");
        let space = " ".repeat(n % 4);
        let tail = "\n".repeat(n % 3);
        let heading = format!(
            "# {word}\n\n### {word}2 %% carve-lint-disable-line heading-level-skip\n{tail}"
        );
        assert!(
            findings(&wrap(heading, n), "heading-level-skip").is_empty(),
            "heading {n}"
        );

        let paragraph = format!("{space}|{word}|b\n");
        assert_eq!(
            findings(&wrap(paragraph, n), "table-shaped-paragraph").len(),
            1,
            "paragraph {n}"
        );

        let near_miss = format!("| {word} | b |\n|\t--- |\t--- |\n| 1 | 2 |\n");
        assert_eq!(
            findings(&wrap(near_miss, n), "table-separator-near-miss").len(),
            1,
            "separator {n}"
        );

        let marker = format!("- {word}\n- %% c\n```\ncode\n");
        assert!(
            findings(&wrap(marker, n), "list-marker-without-blank-line").is_empty(),
            "marker {n}"
        );

        let empty = format!("[{word}]()\n");
        assert_eq!(
            findings(&wrap(empty, n), "empty-link-destination").len(),
            1,
            "empty {n}"
        );

        let unused = format!("{word}.\n\n[{word}]: /u{n}\n");
        assert_eq!(
            findings(&wrap(unused, n), "unused-link-reference-definition").len(),
            1,
            "unused {n}"
        );

        let destination = format!("[{word}](a`b{{c}}d|e^f<g>h)\n");
        assert!(
            findings(&wrap(destination, n), "bare-html-tag").is_empty(),
            "destination {n}"
        );
    }
}
#[test]
fn marker_line_refusal_survives_an_unchanged_ownership_walk() {
    let warnings = lint_carve("- +\n  x\n");
    assert!(warnings.iter().any(|warning| {
        warning.rule == "continuation-marker-refused" && (warning.line, warning.column) == (1, 3)
    }));
}

#[test]
fn an_unclosed_code_fence_in_a_note_reports_its_authored_column() {
    let source = "- [^n]: text\n    more\n\n    %% c\n    ~~~\n    code\nafter [^n]\n";
    let warnings = lint_carve(source);
    assert!(
        warnings.iter().any(|warning| {
            warning.rule == "unclosed-code-fence" && (warning.line, warning.column) == (5, 5)
        }),
        "{warnings:?}"
    );
}

#[test]
fn a_quoted_note_fence_survives_an_erased_comment() {
    let source = "> [^n]: text\n>   more\n\n>   %% c\n>   ~~~\n>   code\nafter [^n]\n";
    assert!(findings(source, "unclosed-code-fence").contains(&(5, 5)));
}

#[test]
fn an_unclosed_comment_fence_reports_its_opener() {
    let source = "before\n\n%%%\nsecret\n\nafter\n";
    let warnings = lint_carve(source);
    assert!(
        warnings.iter().any(|warning| {
            warning.rule == "unclosed-comment-fence" && (warning.line, warning.column) == (3, 1)
        }),
        "{warnings:?}"
    );
}

#[test]
fn comment_fence_lint_respects_owners_and_payloads() {
    for (source, expected) in [
        ("%%%\nsecret\n%%%\n", None),
        ("```\n%%%\n```\n", None),
        ("- item\n  %%%\n  secret\nnext\n", Some((2, 3))),
        ("> %%%\n> secret\nnext\n", Some((1, 3))),
    ] {
        let warnings: Vec<_> = lint_carve(source)
            .into_iter()
            .filter(|warning| warning.rule == "unclosed-comment-fence")
            .map(|warning| (warning.line, warning.column))
            .collect();
        assert_eq!(
            warnings,
            expected.into_iter().collect::<Vec<_>>(),
            "{source:?}"
        );
    }
}

#[test]
fn lint_reproduction_families_survive_generated_variants() {
    for n in 0..1000 {
        let lead = if n % 2 == 0 { "intro\n\n" } else { "" };
        let marker = "%".repeat(3 + n % 4);
        let comment = format!("{lead}{marker}\nsecret {n}\n");
        let comment_line = if lead.is_empty() { 1 } else { 3 };
        assert!(
            findings(&comment, "unclosed-comment-fence").contains(&(comment_line, 1)),
            "comment {n}: {comment:?}"
        );

        let spacer = "\n".repeat(n % 3 + 1);
        let note = format!(
            "- [^n]: text\n    more\n{spacer}    %% c\n    ~~~\n    code {n}\nafter [^n]\n"
        );
        let fence_line = 4 + n % 3 + 1;
        assert!(
            findings(&note, "unclosed-code-fence").contains(&(fence_line, 5)),
            "code {n}: {note:?}"
        );

        let plus = format!("{lead}- +\n  x{n}\n");
        assert!(
            findings(&plus, "continuation-marker-refused").contains(&(comment_line, 3)),
            "plus {n}: {plus:?}"
        );
    }
}
