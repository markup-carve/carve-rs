//! The maintainer rulings of carve#6174 and #6178 (19 November).

use carve::lint::{LintOptions, lint};

type Case = (&'static str, &'static str, &'static [(u32, u32)]);

fn found(source: &str, code: &str) -> Vec<(u32, u32)> {
    lint(source, &LintOptions::default())
        .into_iter()
        .filter(|d| d.code == code)
        .filter_map(|d| d.pos.map(|pos| (pos.start_offset, pos.end_offset)))
        .collect()
}

#[test]
fn line_shape_rules_read_behind_container_prefixes() {
    let cases: &[Case] = &[
        ("- [ ] ___\n", "thematic-break-spelling", &[(6, 9)]),
        ("* ----\n", "thematic-break-spelling", &[(2, 6)]),
        ("[^f]: ---\n", "thematic-break-spelling", &[(6, 9)]),
        (":: term\n: ----\n", "thematic-break-spelling", &[(10, 14)]),
        (">  > ----\n", "thematic-break-spelling", &[(5, 9)]),
        (":: t\n> ---\n", "thematic-break-spelling", &[]),
        (":: t\n  - ----\n", "thematic-break-spelling", &[]),
        ("a. - - -\n", "deprecated-spaced-thematic-break", &[(3, 8)]),
        ("- * * *\n", "deprecated-spaced-thematic-break", &[(2, 7)]),
        ("- - -\n", "deprecated-spaced-thematic-break", &[(0, 5)]),
        ("2) [a]:\n", "empty-reference-definition", &[(3, 7)]),
        (
            ":: term\n: [a]:\n",
            "empty-reference-definition",
            &[(10, 14)],
        ),
    ];
    for (source, code, expected) in cases {
        assert_eq!(found(source, code), *expected, "{source:?} {code}");
    }
}

#[test]
fn lazy_line_offers_its_quote_mark() {
    let code = "block-opener-without-blank-line";
    let cases: &[(&str, &[(u32, u32)])] = &[
        ("    i) > $$x$$\n   > ## H\n", &[(18, 19)]),
        ("[^f]: > [^m]: key [ref][]\n> [\n", &[(26, 27)]),
        ("10. %%#region\n     > :\n> [ ]: u\n", &[(23, 24)]),
        ("- > a\n> ## H\n", &[(6, 7)]),
        ("- > a\n  > ## H\n", &[(10, 12)]),
        ("> a\n> ## H\n", &[(6, 8)]),
    ];
    for (source, expected) in cases {
        assert_eq!(found(source, code), *expected, "{source:?}");
    }
}
