use carve::lint::{LintOptions, lint};

#[test]
fn djot_underscore_respects_both_word_boundaries() {
    let mut options = LintOptions::default();
    options
        .rules
        .insert("djot-emphasis-underscore".into(), "on".into());
    for source in ["x_a_\n", "3_a_\n", "__a_\n", "x_a_ x_a_\n"] {
        assert!(
            !lint(source, &options)
                .iter()
                .any(|entry| entry.code == "djot-emphasis-underscore"),
            "{source:?}"
        );
    }
    assert!(
        lint("_a_\n", &options)
            .iter()
            .any(|entry| entry.code == "djot-emphasis-underscore")
    );
    assert!(
        lint("_a\\_b_\n", &options)
            .iter()
            .any(|entry| entry.code == "djot-emphasis-underscore")
    );
}

fn lines(source: &str) -> Vec<u32> {
    lint(source, &LintOptions::default())
        .into_iter()
        .filter(|finding| finding.code == "bare-html-tag")
        .filter_map(|finding| finding.pos.map(|pos| pos.start_line))
        .collect()
}

#[test]
fn c2_b_215_c2_b_216_line_and_document_scopes() {
    assert_eq!(
        lines("<x> %% carve-lint-disable-line bare-html-tag\n"),
        Vec::<u32>::new()
    );
    assert_eq!(
        lines("%% carve-lint-disable-next-line bare-html-tag\n<x>\n"),
        Vec::<u32>::new()
    );
    assert_eq!(
        lines(
            "%% carve-lint-disable bare-html-tag\n<x>\n%% carve-lint-enable bare-html-tag\n<x>\n"
        ),
        [4]
    );
    assert_eq!(
        lines("%%  carve-lint-disable-next-line bare-html-tag\n<x>\n"),
        [2]
    );
    assert_eq!(
        lines("```\n%% carve-lint-disable-next-line bare-html-tag\n```\n<x>\n"),
        [4]
    );
}

#[test]
#[ignore = "candidate 12 lint messages are held outside the core stage"]
fn c2_w_106_broken_crossref_message_variants() {
    let cases = [
        (
            "{#Title}\n= Heading\n\n</#title>\n",
            "the heading \"Title\" differs only in case",
        ),
        ("{#target}\nText\n\n</#target>\n", "which is on a paragraph"),
        ("</#missing>\n", "has no matching heading id"),
    ];
    for (source, fragment) in cases {
        let findings = lint(source, &LintOptions::default());
        let finding = findings
            .iter()
            .find(|d| d.code == "broken-crossref")
            .expect("crossref");
        assert!(finding.message.contains(fragment), "{:?}", finding.message);
        assert!(finding.path.is_some() && finding.pos.is_some());
    }
}

#[test]
#[ignore = "candidate 12 lint messages are held outside the core stage"]
fn c2_i_080_c2_i_081_unresolved_reference_message_variants() {
    let cases = [
        (
            "[a][foo]\n\n[FOO]: /u\n",
            "Reference link",
            "the label FOO differs only in case",
        ),
        (
            "![a][foo]\n\n[FOO]: /u\n",
            "Reference image",
            "the label FOO differs only in case",
        ),
        (
            "[a][foo]\n",
            "Reference link",
            "has no matching link definition",
        ),
        (
            "![a][foo]\n",
            "Reference image",
            "has no matching link definition",
        ),
    ];
    for (source, subject, fragment) in cases {
        let findings = lint(source, &LintOptions::default());
        let finding = findings
            .iter()
            .find(|d| d.code == "unresolved-reference-link")
            .expect("reference");
        assert!(
            finding.message.contains(subject) && finding.message.contains(fragment),
            "{:?}",
            finding.message
        );
        assert!(finding.path.is_some() && finding.pos.is_some());
    }
}

#[test]
fn c2_b_195_comment_fence_with_attached_attributes() {
    let findings = lint("{.c}\n%%%\nhidden\n%%%\npara", &LintOptions::default());
    assert!(
        !findings
            .iter()
            .any(|d| d.code == "comment-fence-closer-with-text")
    );
}

#[test]
fn c2_b_154_heading_continues_across_comment() {
    let findings = lint("= Title\n%% note\n  continues", &LintOptions::default());
    assert!(
        findings
            .iter()
            .any(|d| d.code == "deprecated-indented-line-under-heading")
    );
}

#[test]
fn c2_i_049_only_double_star_runs_are_markdown_strong() {
    let findings = lint("**bold** and ***x***", &LintOptions::default());
    assert_eq!(
        findings
            .iter()
            .filter(|d| d.code == "markdown-strong-double-star")
            .count(),
        1
    );
}

#[test]
fn c2_w_137_trailing_and_standalone_table_comments() {
    for source in ["| a |\n%% x\n+ b |\n", "| a | %% x\n+ b |\n"] {
        let findings = lint(source, &LintOptions::default());
        assert!(
            findings
                .iter()
                .any(|d| d.code == "comment-splits-table-row"),
            "{source:?}"
        );
    }
}

#[test]
fn c2_b_012_lazy_list_line_is_not_a_new_block() {
    let findings = lint("- b\n d", &LintOptions::default());
    assert!(
        !findings
            .iter()
            .any(|d| d.code == "list-item-body-below-content-column")
    );
}

#[test]
fn c2_b_090_glued_valid_attributes_are_not_malformed() {
    let findings = lint("{a}{b}\n", &LintOptions::default());
    assert!(!findings.iter().any(|d| d.code == "block-marker-as-text"));
    assert!(
        findings
            .iter()
            .any(|d| d.code == "unattached-block-attribute")
    );
}

#[test]
fn c2_w_111_deprecated_bare_marks_do_not_start_inside_runs() {
    let findings = lint("__W_", &LintOptions::default());
    assert!(
        !findings
            .iter()
            .any(|d| d.code == "deprecated-bare-underline")
    );
    let findings = lint("_t__", &LintOptions::default());
    assert!(
        findings
            .iter()
            .any(|d| d.code == "deprecated-bare-underline")
    );
}

#[test]
fn c2_b_010_lone_carriage_return_is_not_a_line_start() {
    let findings = lint("\r>word", &LintOptions::default());
    assert!(
        !findings
            .iter()
            .any(|d| d.code == "blockquote-marker-without-space")
    );
}

#[test]
fn c2_w_106_term_atom_crossing_lines_is_multiline() {
    let findings = lint(":: [^\n[]", &LintOptions::default());
    assert!(
        findings
            .iter()
            .any(|d| d.code == "description-term-multi-line")
    );
}
