//! A `+` no container claims is a letter under PART 0 S2, so lint stays quiet
//! whatever follows it (markup-carve/carve#4648).

use carve::lint_carve;

fn findings(source: &str) -> Vec<(usize, usize, String)> {
    lint_carve(source)
        .into_iter()
        .filter(|warning| warning.rule == "continuation-marker-refused")
        .map(|warning| (warning.line, warning.column, warning.message))
        .collect()
}

#[test]
fn a_plus_read_as_a_letter_stays_quiet() {
    for source in [
        "q\n+\n- s\n",
        "q\n+\n| a |\n",
        "q\n+\n# h\n",
        "q\n+\n```\nx\n```\n",
        "q\n+\n> x\n",
        "+\n# h\n",
        "+\n***\n",
        "+\n+\n# h\n",
        "> q\n>   +\n> para\n",
        "> > - a\n>   +\n>   - b\n",
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
