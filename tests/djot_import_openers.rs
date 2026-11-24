//! Djot import finds the opener a closing token matches without walking the stack of unclosed
//! openers: a paragraph of thousands of unclosed `[` or `{=` imports in linear time.

use carve::api::{Carve, ImportFormat, ImportOptions, LossOptions, WriteOptions};
use std::time::{Duration, Instant};

fn import(source: &str) -> String {
    let carve = Carve::new();
    let imported = carve
        .import_from(
            source,
            ImportFormat::Djot,
            &ImportOptions::default(),
            &LossOptions::default(),
        )
        .expect("Djot import succeeds");
    carve
        .write_with_diagnostics(&imported.tree, &WriteOptions::default())
        .expect("the imported tree writes")
        .output
}

#[test]
fn openers_still_pair_with_their_closers() {
    assert_eq!(import("[a](u) {=b=} _c_ ^[x\n"), "[a](u) {=b=} /c/ ^[x\n");
    assert_eq!(import("[[[a]\n"), "[[[a]\n");
    assert_eq!(import("_a [[ _ b_\n"), "/a [[ _ b/\n");
    assert_eq!(import("{_ {* {+ x+} *} _}\n"), "{/ {* {+ x+} *} /}\n");
}

fn time_of(source: &str) -> Duration {
    (0..3)
        .map(|_| {
            let t = Instant::now();
            std::hint::black_box(import(source));
            t.elapsed()
        })
        .min()
        .unwrap_or_default()
}

fn assert_linear(name: &str, shape: impl Fn(usize) -> String) {
    let sizes = [2000, 4000, 8000];
    let ratios = || -> Vec<f64> {
        let times: Vec<f64> = sizes
            .iter()
            .map(|&size| time_of(&shape(size)).as_secs_f64())
            .collect();
        times.windows(2).map(|w| w[1] / w[0]).collect()
    };
    let first = ratios();
    if first.iter().all(|&r| r < 3.0) {
        return;
    }
    let second = ratios();
    assert!(
        second.iter().all(|&r| r < 3.0),
        "{name}: time ratios per doubling {first:.2?} and {second:.2?}, limit 3.0"
    );
}

#[test]
fn unclosed_openers_are_linear() {
    // Every token used to scan the opener stack: doubling the run took about four times as long.
    for opener in ["[", "![", "^[", "{_", "{*", "{+", "{="] {
        assert_linear(opener, |n| format!("{}\n", opener.repeat(n)));
    }
    assert_linear("mixed", |n| format!("{}\n", "[{=![".repeat(n / 3)));
}

#[test]
fn refused_closers_below_unclosed_openers_are_linear() {
    // A closer after whitespace finds the bottom `_` and is refused, once per token.
    assert_linear("refused", |n| format!("_a {} {}\n", "[".repeat(n), " _".repeat(n)));
}
