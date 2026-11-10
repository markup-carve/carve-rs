use carve::lint_carve;

#[test]
fn refused_own_line_markers_hint_across_hosts_and_stops() {
    let cases = [
        ("see[^a]\n\n[^a]: +\n+\nx\n", vec![(4, 1)]),
        ("- a\n+\n\nx\n", vec![(2, 1)]),
        ("> - a\n> +\n>\n> b\n", vec![(2, 3)]),
        ("> q\n+\n+\n", vec![(2, 1), (3, 1)]),
    ];
    for (source, expected) in cases {
        let actual: Vec<_> = lint_carve(source).into_iter()
            .filter(|row| row.rule == "refused-continuation-marker")
            .map(|row| {
                assert!(row.message.contains("attaches nothing"));
                assert!(row.message.contains("removed"));
                (row.line, row.column)
            }).collect();
        assert_eq!(actual, expected, "{source:?}");
    }
    for source in ["see[^a]\n\n[^a]: +\nx\n", "q\n+\n- s\n", "- a\n%% carve-lint-disable-next-line refused-continuation-marker\n+\n\nx\n"] {
        assert!(!lint_carve(source).iter().any(|row| row.rule == "refused-continuation-marker"), "{source:?}");
    }
}

#[test]
fn generated_blank_stop_variants_report_the_refused_offer() {
    let mut checked = 0;
    for word in 0..250 {
        let sources = [
            format!("- item{word}\n+\n\ntail{word}\n"),
            format!("> quote{word}\n+\n\ntail{word}\n"),
            format!(":: term{word}\n: definition{word}\n+\n\ntail{word}\n"),
            format!("see[^a]\n\n[^a]: note{word}\n+\n\ntail{word}\n"),
        ];
        for source in sources {
            let count = lint_carve(&source).into_iter()
                .filter(|row| row.rule == "refused-continuation-marker").count();
            assert_eq!(count, 1, "{source:?}");
            checked += 1;
        }
    }
    assert_eq!(checked, 1000);
}
