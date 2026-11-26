//! The hold-list rule of `support/holds.rs` that every spec-case runner uses.
#[path = "support/holds.rs"]
mod holds;

use holds::{Gate, expect_held};
use std::panic::catch_unwind;

fn judged(
    holds: &[(&str, &str)],
    results: &[(&str, bool)],
) -> (usize, usize, Vec<String>, Vec<String>) {
    let mut gate = Gate::new("test", holds);
    for &(id, ok) in results {
        gate.check(id, ok, || format!("{id} failed"));
    }
    let j = gate.judge();
    (j.pass, j.held, j.failures, j.stale)
}

#[test]
fn an_unheld_failure_fails_the_run() {
    let (pass, held, failures, stale) = judged(&[], &[("CC-1", true), ("CC-2", false)]);
    assert_eq!((pass, held, stale.len()), (1, 0, 0));
    assert_eq!(failures, ["=== CC-2\nCC-2 failed"]);
}

#[test]
fn a_held_failure_passes() {
    let (pass, held, failures, stale) =
        judged(&[("CC-2", "carve#1")], &[("CC-1", true), ("CC-2", false)]);
    assert_eq!((pass, held, failures.len(), stale.len()), (1, 1, 0, 0));
}

#[test]
fn a_held_case_that_passes_is_stale() {
    let (_, _, failures, stale) = judged(&[("CC-1", "carve#1")], &[("CC-1", true)]);
    assert!(failures.is_empty());
    assert_eq!(
        stale,
        ["=== CC-1: held (carve#1) but passes; drop the hold"]
    );
}

#[test]
fn a_held_case_the_runner_never_sees_is_stale() {
    let (_, _, _, stale) = judged(&[("CC-9", "carve#1")], &[("CC-1", true)]);
    assert_eq!(
        stale,
        ["=== CC-9: held (carve#1) but not a case of test; drop the hold"]
    );
}

#[test]
fn one_failing_check_fails_the_case() {
    let (pass, _, failures, _) = judged(&[], &[("CC-1", true), ("CC-1", false)]);
    assert_eq!((pass, failures.len()), (0, 1));
}

#[test]
fn a_panicking_case_body_is_a_failure() {
    let mut gate = Gate::new("test", &[("CC-2", "carve#1")]);
    gate.case("CC-1", || {});
    gate.case("CC-2", || panic!("boom"));
    let j = gate.judge();
    assert_eq!(
        (j.pass, j.held, j.failures.len(), j.stale.len()),
        (1, 1, 0, 0)
    );
}

#[test]
#[should_panic(expected = "CC-1 is held twice")]
fn a_case_is_held_once() {
    Gate::new("test", &[("CC-1", "a"), ("CC-1", "b")]);
}

#[test]
fn expect_held_runs_a_case_that_is_not_held() {
    expect_held("CC-1", &[], || {});
    assert!(catch_unwind(|| expect_held("CC-1", &[], || panic!("fails"))).is_err());
}

#[test]
fn expect_held_keeps_a_held_case_failing() {
    expect_held("CC-1", &[("CC-1", "carve#1")], || panic!("still fails"));
    assert!(catch_unwind(|| expect_held("CC-1", &[("CC-1", "carve#1")], || {})).is_err());
    assert!(catch_unwind(|| expect_held("CC-1", &[("CC-2", "carve#1")], || {})).is_err());
}
