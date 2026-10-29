//! A document-level `+` no container claims is reported only when a block follows
//! it; before paragraph text it is ordinary text (markup-carve/carve#3421).

use carve::lint_carve;

fn findings(source: &str) -> Vec<(usize, usize, String)> {
    lint_carve(source)
        .into_iter()
        .filter(|warning| warning.rule == "continuation-marker-refused")
        .map(|warning| (warning.line, warning.column, warning.message))
        .collect()
}

#[test]
fn a_block_below_a_document_plus_reports_it() {
    for (source, line) in [
        ("q\n+\n- s\n", 2),
        ("q\n+\n| a |\n", 2),
        ("q\n+\n# h\n", 2),
        ("q\n+\n```\nx\n```\n", 2),
        ("q\n+\n> x\n", 2),
        ("+\n# h\n", 1),
        ("+\n***\n", 1),
        ("+\n+\n# h\n", 2),
    ] {
        let found = findings(source);
        assert_eq!(found.len(), 1, "{source:?}: {found:?}");
        assert_eq!((found[0].0, found[0].1), (line, 1), "{source:?}");
        assert!(
            found[0].2.contains("no open container claims it")
                && found[0].2.contains("remains text in the document"),
            "{source:?}: {}",
            found[0].2
        );
    }
}

#[test]
#[ignore = "rule A ledger: carve-rs#3524"]
fn paragraph_text_a_comment_or_an_attribute_below_it_stays_quiet() {
    for source in [
        "+\npara\n",
        "q\n+\npara\n",
        "q\n+\n%% c\n",
        "q\n+\n{.c}\npara\n",
        "+\n",
        "  +\n# h\n",
        "+\n  # h\n",
        "+\n: d\n",
    ] {
        assert_eq!(findings(source), Vec::new(), "{source:?}");
    }
}

#[test]
fn an_unclaimed_plus_in_a_quote_names_the_quote() {
    let found = findings("> q\n>   +\n> para\n");
    assert_eq!(found.len(), 1);
    assert!(
        found[0].2.ends_with("It remains text in the block quote."),
        "{}",
        found[0].2
    );
}
