//! fmt time linear in the input: the shapes are repeated at n, 2n, 4n and 8n,
//! and each doubling may take at most 2.6 times as long, as in `linearity.rs`.

use carve::api::{Carve, WriteOptions};
use std::time::{Duration, Instant};

const LIMIT: f64 = 2.6;
const FLOOR: Duration = Duration::from_millis(4);

fn time_of(carve: &Carve, src: &str) -> Duration {
    let opts = WriteOptions::default();
    (0..5)
        .map(|_| {
            let t = Instant::now();
            let _ = std::hint::black_box(carve.format_with_diagnostics(src, &opts));
            t.elapsed()
        })
        .min()
        .unwrap_or_default()
}

fn ratios(carve: &Carve, unit: &str) -> Vec<f64> {
    let mut base = 1;
    while time_of(carve, &unit.repeat(base)) < FLOOR && base < 1 << 14 {
        base *= 2;
    }
    let times: Vec<f64> = [1, 2, 4, 8]
        .iter()
        .map(|k| time_of(carve, &unit.repeat(base * k)).as_secs_f64())
        .collect();
    times.windows(2).map(|w| w[1] / w[0]).collect()
}

/// A run of comment lines is one group; the group end and the sibling loop
/// used to walk the whole run again for every comment.
#[test]
fn comment_lines() {
    let carve = Carve::new();
    let first = ratios(&carve, "%%\n");
    if first.iter().all(|&r| r < LIMIT) {
        return;
    }
    let second = ratios(&carve, "%%\n");
    assert!(
        second.iter().all(|&r| r < LIMIT),
        "comment lines: time ratios per doubling {first:.2?} and {second:.2?}, limit {LIMIT}"
    );
}
