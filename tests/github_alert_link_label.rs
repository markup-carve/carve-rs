use carve::lint_carve;

#[test]
fn linked_alert_labels_do_not_claim_to_stay_literal() {
    for source in [
        "> [!NOTE][]\n> x\n\n[!NOTE]: /u\n",
        "> [!NOTE][!NOTE]\n> x\n\n[!NOTE]: /u\n",
        "> [!NOTE](/u)\n> x\n",
        "> > [!NOTE][]\n> > x\n\n[!NOTE]: /u\n",
    ] {
        assert!(
            lint_carve(source)
                .iter()
                .all(|warning| warning.rule != "github-alert-marker"),
            "{source:?}"
        );
    }
}

#[test]
fn a_literal_alert_label_is_still_reported() {
    let warning = lint_carve("> [!NOTE]\n> x\n")
        .into_iter()
        .find(|warning| warning.rule == "github-alert-marker")
        .expect("literal label has a warning");
    assert_eq!((warning.line, warning.column), (1, 3));
}

#[test]
fn a_literal_label_after_a_linked_label_is_reported() {
    let warnings: Vec<_> = lint_carve("> [!NOTE](/u) [!TIP]\n")
        .into_iter()
        .filter(|warning| warning.rule == "github-alert-marker")
        .collect();
    assert_eq!(warnings.len(), 1);
    assert_eq!((warnings[0].line, warnings[0].column), (1, 15));
}

#[test]
fn linked_alert_labels_stay_quiet_across_hosts_and_siblings() {
    let mut count = 0;
    for label in ["NOTE", "TIP", "IMPORTANT", "WARNING", "CAUTION"] {
        for depth in 1..=4 {
            let prefix = "> ".repeat(depth);
            for form in 0..5 {
                for sibling in 0..10 {
                    let marker = match form {
                        0 => format!("[!{label}][]"),
                        1 => format!("[!{label}][!{label}]"),
                        2 => format!("[!{label}](/u)"),
                        3 => format!("[!{label}](</u>)"),
                        _ => format!("[!{label}](/u \"title\")"),
                    };
                    let source =
                        format!("{prefix}{marker}\n{prefix}neighbor {sibling}\n\n[!{label}]: /u\n");
                    assert!(
                        lint_carve(&source)
                            .iter()
                            .all(|warning| warning.rule != "github-alert-marker"),
                        "{source:?}"
                    );
                    count += 1;
                }
            }
        }
    }
    assert_eq!(count, 1_000);
}
