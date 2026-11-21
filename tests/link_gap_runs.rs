//! carve-rs#4547: `link-gap-before-destination` takes the catalog row's matches left to right without
//! overlap (C2-I-069, C2-T-019), so a finding starts at the first `[` of a run.

use carve::lint::{LintOptions, lint};

fn ranges(src: &str) -> Vec<(u32, u32)> {
    lint(src, &LintOptions::default())
        .iter()
        .filter(|d| d.code == "link-gap-before-destination")
        .filter_map(|d| d.pos.map(|pos| (pos.start_offset, pos.end_offset)))
        .collect()
}

#[test]
fn a_finding_starts_at_the_first_bracket_of_its_match() {
    for (src, expected) in [
        ("[[a] (x)\n", vec![(0, 8)]),
        ("[[[a] (x)\n", vec![(0, 9)]),
        ("[b[a] (x)\n", vec![(0, 9)]),
        ("[a [b] (x)\n", vec![(0, 10)]),
        ("[[a] (x [b] (y)\n", vec![(0, 15)]),
        ("\\[[a] (x)\n", vec![(2, 9)]),
        ("p [a] (x) [b] (y)\n", vec![(2, 9), (10, 17)]),
        ("[a] ( ) [b] (y)\n", vec![(8, 15)]),
        ("[a]\t(x)\n", vec![(0, 7)]),
        ("[a] (x\n", vec![]),
    ] {
        assert_eq!(ranges(src), expected, "{src:?}");
    }
}

#[test]
fn a_long_bracket_run_reports_once_from_its_start() {
    let src = format!("{}a] (x)\n", "[".repeat(50_000));
    assert_eq!(ranges(&src), vec![(0, 50_006)]);
}
