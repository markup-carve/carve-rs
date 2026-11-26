//! Time linear in the input: parse, resolve and HTML at sizes n, 2n, 4n and 8n
//! for the benchmark documents (tests/data/bench, from carve-js `next2`) and
//! for stress shapes. Each doubling may take at most 2.6 times as long.

use std::fmt::Write as _;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const LIMIT: f64 = 2.6;
/// The shapes are timed one at a time: parallel tests would time each other.
static SERIAL: Mutex<()> = Mutex::new(());
/// Below this a timing is mostly noise: the document is repeated until 1n takes longer.
const FLOOR: Duration = Duration::from_millis(4);

fn time_of(src: &str) -> Duration {
    let opts = carve::Options::default();
    (0..7)
        .map(|_| {
            let t = Instant::now();
            std::hint::black_box(carve::render(src, &opts));
            t.elapsed()
        })
        .min()
        .unwrap_or_default()
}

/// The time ratio of each doubling, the size of 1n raised until it takes `FLOOR`.
fn ratios(make: &dyn Fn(usize) -> String) -> Vec<f64> {
    let mut base = 1;
    while time_of(&make(base)) < FLOOR && base < 1 << 12 {
        base *= 2;
    }
    let times: Vec<f64> = [1, 2, 4, 8]
        .iter()
        .map(|k| time_of(&make(base * k)).as_secs_f64())
        .collect();
    times.windows(2).map(|w| w[1] / w[0]).collect()
}

fn check(name: &str, make: &dyn Fn(usize) -> String) {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // A loaded machine can slow one run down: a shape fails only when two
    // measurements in a row exceed the limit.
    let first = ratios(make);
    if first.iter().all(|&r| r < LIMIT) {
        return;
    }
    let second = ratios(make);
    assert!(
        second.iter().all(|&r| r < LIMIT),
        "{name}: time ratios per doubling {first:.2?} and {second:.2?}, limit {LIMIT}"
    );
}

fn repeat(unit: &str, k: usize) -> String {
    let unit = if unit.ends_with('\n') {
        unit.to_string()
    } else {
        format!("{unit}\n")
    };
    vec![unit; k].join("\n")
}

macro_rules! document {
    ($test:ident, $file:literal) => {
        #[test]
        fn $test() {
            let unit = include_str!(concat!("data/bench/", $file));
            check($file, &|k| repeat(unit, k));
        }
    };
}

document!(commonmark_core, "commonmark-core.crv");
document!(prose, "prose.crv");
document!(lists_tables, "lists-tables.crv");
document!(reference_heavy, "reference-heavy.crv");

/// The four documents in a row, the unit the 1 MB benchmark document repeats.
#[test]
fn all_four() {
    let unit = [
        include_str!("data/bench/commonmark-core.crv"),
        include_str!("data/bench/prose.crv"),
        include_str!("data/bench/lists-tables.crv"),
        include_str!("data/bench/reference-heavy.crv"),
    ]
    .join("\n");
    check("all four", &|k| repeat(&unit, k));
}

/// Quotes 150 deep around a list item with nested emphasis, repeated.
#[test]
fn deep_nesting() {
    let unit = format!("{}- *a _b {{=c=}} d_ e* [f](g)\n", "> ".repeat(150));
    check("deep nesting", &|k| repeat(&unit, 8 * k));
}

/// One paragraph of a single delimiter character, and one of unmatched openers.
#[test]
fn delimiter_runs() {
    check("one delimiter", &|k| "*".repeat(2000 * k));
    check("openers", &|k| "*a _b [c ".repeat(300 * k));
}

/// A table of few rows and very many cells.
#[test]
fn wide_table() {
    check("wide table", &|k| {
        let row = format!("|{}\n", "a *b* |".repeat(200 * k));
        row.repeat(4)
    });
}

/// Many references with their definitions, footnotes and headings that share a text.
#[test]
fn many_references() {
    check("references", &|k| {
        let mut s = String::new();
        for i in 0..100 * k {
            let _ = write!(
                s,
                "# Part\n\nSee [x{i}][], [the part][Part] and a note[^n{i}].\n\n"
            );
        }
        for i in 0..100 * k {
            let _ = write!(s, "[x{i}]: /u/{i}\n[^n{i}]: Note {i}.\n");
        }
        s
    });
}

/// One very long line of mixed inline content.
#[test]
fn long_line() {
    check("long line", &|k| {
        "a *b* `c` [d](e) \"f\" -- g... ".repeat(200 * k)
    });
}

#[test]
fn c2_b_024_lazy_comments_in_line_block_stay_linear() {
    check("lazy line-block comments", &|k| {
        format!("- ::: |\n  a\n{}", "%% c\nb\n".repeat(1000 * k))
    });
}

#[test]
fn c2_b_123_lazy_comments_after_comment_blocks_stay_linear() {
    check("lazy comments after comment blocks", &|k| {
        let n = 500 * k;
        format!(
            "- ::: |\n  a\n{}{}",
            "  %%%\n  %%%\n".repeat(n),
            "%% x\n".repeat(n)
        )
    });
}

#[test]
fn c2_h_201_derived_text_after_budget_stays_linear() {
    check("derived text after budget", &|k| {
        let heading = "H".repeat(1000 * k);
        format!(
            "{{#target}}\n= {heading}\n\n{}",
            "</#target> ".repeat(1000 * k)
        )
    });
}

/// carve#6411: past the depth cap each `*` looked for a `]` in the rest of the text.
#[test]
fn c2_i_118_capped_openers_before_brackets_stay_linear() {
    check("capped openers before brackets", &|k| {
        let n = 150 * k;
        format!("{}^*[*x](u){}\n", "^*[*[[*".repeat(n), "^]]](u)".repeat(n))
    });
}

/// carve#6543: each `]` folded its whole content as a label, though no definition could match it.
#[test]
fn c2_i_072_nested_brackets_without_definitions_stay_linear() {
    check("nested brackets without definitions", &|k| {
        let depth = 24 * k.min(8);
        let pair = format!("{}b{}", "[a ".repeat(depth), "]".repeat(depth));
        vec![pair; 400].join("\n\n")
    });
}
