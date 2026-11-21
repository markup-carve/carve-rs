//! Lint time linear in the input (carve-rs#4359): the shapes are repeated at
//! n, 2n, 4n and 8n, and each doubling may take at most 2.6 times as long.

use carve::lint::{LintOptions, lint};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const LIMIT: f64 = 2.6;
static SERIAL: Mutex<()> = Mutex::new(());
const FLOOR: Duration = Duration::from_millis(4);

fn time_of(src: &str, opts: &LintOptions) -> Duration {
    (0..5)
        .map(|_| {
            let t = Instant::now();
            std::hint::black_box(lint(src, opts));
            t.elapsed()
        })
        .min()
        .unwrap_or_default()
}

fn ratios(unit: &str, opts: &LintOptions) -> Vec<f64> {
    let mut base = 1;
    // At tiny sizes the fixed call setup dominates and makes adjacent ratios
    // unstable; measure the scaling region the issue describes.
    while (unit.len() * base < 2048 || time_of(&unit.repeat(base), opts) < FLOOR) && base < 1 << 12
    {
        base *= 2;
    }
    let times: Vec<f64> = [1, 2, 4, 8]
        .iter()
        .map(|k| time_of(&unit.repeat(base * k), opts).as_secs_f64())
        .collect();
    times.windows(2).map(|w| w[1] / w[0]).collect()
}

fn check(name: &str, unit: &str) {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let opts = LintOptions {
        platforms: vec!["github".into()],
        ..LintOptions::default()
    };
    // A loaded machine can slow one run down: a shape fails only when two
    // measurements in a row exceed the limit.
    let first = ratios(unit, &opts);
    if first.iter().all(|&r| r < LIMIT) {
        return;
    }
    let second = ratios(unit, &opts);
    assert!(
        second.iter().all(|&r| r < LIMIT),
        "{name}: time ratios per doubling {first:.2?} and {second:.2?}, limit {LIMIT}"
    );
}

#[test]
fn mixed_document() {
    check(
        "mixed",
        "- [ ] task one with *strong* text\n- [x] done item\n\nA paragraph with :wiki[Page] and a [link](https://x.y) and more words here.\n\n{{ part.crv }}\n\n> quote [ ] text\n\n",
    );
}

#[test]
fn quotes_with_findings() {
    check("quotes", "> quote [ ] text\n\n");
}

#[test]
fn include_lines() {
    check("includes", "{{ part.crv }}\n\n");
}

#[test]
fn ordered_list_warnings() {
    check("ordered list", " 9. a\n10. b\n\n");
}

#[test]
fn control_comments() {
    check(
        "controls",
        "%% carve-lint-disable-next-line blank-reference-label\nText [ ] here.\n\n",
    );
}

#[test]
fn captions_next_to_tables() {
    check("captions", "Table: numbers\n| a | b |\n\n");
}

#[test]
fn footnotes() {
    check("footnotes", "Text[^n].\n\n[^n]: note\n  - one\n- x\n\n");
}

#[test]
fn bracket_runs() {
    check("open brackets", "[");
    check("brackets before one gap", "[[[[a] ( ");
    check("gaps without a closing parenthesis", "[a] (");
    check("blank destinations", "[a] (    ");
    check("labels sharing a destination", "[[a] (x) ");
    check("destinations spanning labels", "[[a] (x [b] ");
}
