use carve::lint::{LintOptions, lint};

fn fence_findings(opener: &str) -> Vec<carve::diagnostic::Diagnostic> {
    let run = opener.bytes().take_while(|&b| b == b'`').count();
    let source = format!("{opener}\nx\n{}\n", "`".repeat(run));
    lint(&source, &LintOptions::default())
        .into_iter()
        .filter(|d| d.code == "fence-title-syntax")
        .collect()
}

#[test]
fn whitespace_only_bare_fence_tails_are_not_further_words() {
    for run in ["```", "````"] {
        for tail in [" ", "  ", "   ", " \t", "  \t", "\t ", "\t\t"] {
            let opener = format!("{run}{tail}");
            let findings = fence_findings(&opener);
            if tail.starts_with('\t') {
                assert_eq!(findings.len(), 1, "{opener:?}");
                assert_eq!(
                    findings[0].pos.as_ref().map(|p| p.start_column),
                    Some((run.len() + 1) as u32)
                );
            } else {
                assert!(findings.is_empty(), "{opener:?}: {findings:?}");
            }
        }
    }
}

#[test]
fn a_word_after_two_spaces_stays_reported_at_the_word() {
    for run in ["```", "````"] {
        let findings = fence_findings(&format!("{run}  word"));
        assert_eq!(findings.len(), 1);
        assert_eq!(
            findings[0].pos.as_ref().map(|p| p.start_column),
            Some((run.len() + 3) as u32)
        );
    }
}
