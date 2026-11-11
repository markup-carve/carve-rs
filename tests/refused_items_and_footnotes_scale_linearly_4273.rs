//! Lint of many refused `+` items and rendering of many footnote definitions
//! run in linear time (carve-rs#4273, PART 9 §9 LINEAR TIME).

use std::time::{Duration, Instant};

fn best_time(run: impl Fn()) -> Duration {
    (0..3)
        .map(|_| {
            let start = Instant::now();
            run();
            start.elapsed()
        })
        .min()
        .unwrap()
}

fn refused_items(n: usize) -> String {
    "- +\n\n".repeat(n)
}

fn footnote_definitions(n: usize) -> String {
    (0..n).map(|i| format!("[^a{i}]: x\n\n")).collect()
}

fn assert_linear(name: &str, run: impl Fn(&str), shape: fn(usize) -> String) {
    let (small, large) = (shape(1000), shape(4000));
    let small = best_time(|| run(&small));
    let large = best_time(|| run(&large));
    // 4x the input: linear is about 4x, the quadratic paths were 16x.
    assert!(
        large <= small * 9 + Duration::from_millis(50),
        "{name}: 1000 items {small:?}, 4000 items {large:?}"
    );
}

#[test]
fn refused_item_lint_scales_linearly() {
    assert_linear(
        "lint",
        |source| assert!(!carve::lint_carve(source).is_empty()),
        refused_items,
    );
}

#[test]
fn footnote_definitions_scale_linearly() {
    assert_linear(
        "html",
        |source| assert!(carve::to_html(source).is_empty()),
        footnote_definitions,
    );
    assert_linear(
        "lint",
        |source| assert!(!carve::lint_carve(source).is_empty()),
        footnote_definitions,
    );
}

#[test]
fn refused_items_and_footnotes_keep_their_output() {
    let warnings = carve::lint_carve(&refused_items(3));
    assert_eq!(warnings.len(), 3);
    assert!(warnings
        .iter()
        .all(|w| w.rule == "continuation-marker-refused"));
    assert_eq!(
        warnings.iter().map(|w| w.line).collect::<Vec<_>>(),
        [1, 3, 5]
    );
    let warnings = carve::lint_carve(&footnote_definitions(3));
    assert_eq!(
        warnings
            .iter()
            .map(|w| (w.rule, w.line, w.start))
            .collect::<Vec<_>>(),
        [
            ("unused-footnote-definition", 1, 0),
            ("unused-footnote-definition", 3, 10),
            ("unused-footnote-definition", 5, 20),
        ]
    );
}
