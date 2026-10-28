use carve::lint_carve;

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
