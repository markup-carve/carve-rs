//! Which walk sets the stack floor for a document at the nesting cap.
//!
//! A measurement, not an assertion, so it is `#[ignore]`d:
//!
//!     cargo test --release --test stack_floor_attribution -- --ignored --nocapture
//!
//! Every probe runs in a CHILD PROCESS. A Rust stack overflow aborts the
//! process rather than unwinding, so a thread that overflows takes the test
//! binary with it - measuring this in-process reports nothing and kills the
//! run, which is how the first version of this file failed.
use std::process::Command;
use std::thread;

const CAP: usize = 200;
const DEPTH: &str = "CARVE_STACK_FLOOR_DEPTH";
const CASE: &str = "CARVE_STACK_FLOOR_CASE";
const STACK: &str = "CARVE_STACK_FLOOR_KIB";

fn deep_source() -> String {
    let depth: usize = std::env::var(DEPTH)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(CAP);
    format!(
        "{}deep\n{}",
        ":::: note\n".repeat(depth),
        "::::\n".repeat(depth)
    )
}

/// The child half: run one case on a thread of the requested size.
fn run_case(case: String, kib: usize) {
    let source = deep_source();
    let worker = thread::Builder::new()
        .stack_size(kib * 1024)
        .spawn(move || {
            assert!(run_one(&source, &case));
        })
        .expect("spawn");
    worker.join().expect("join");
}

/// Each case leaks its tree unless the case IS the drop, so a recursive Drop
/// cannot be charged to the walk being measured.
fn run_one(source: &str, case: &str) -> bool {
    match case {
        "parse" => {
            let doc = carve::parse(source);
            std::mem::forget(doc);
        }
        "parse+drop" => {
            let doc = carve::parse(source);
            drop(doc);
        }
        "parse+to_json" => {
            let doc = carve::parse(source);
            let json = carve::try_to_json(&doc).expect("encodes");
            assert!(!json.is_empty());
            std::mem::forget(doc);
        }
        "to_html" => {
            let html = carve::to_html(source);
            assert!(!html.is_empty());
        }
        other => panic!("unknown case {other}"),
    }
    true
}

fn fits_at(case: &str, kib: usize, depth: usize) -> bool {
    let exe = std::env::current_exe().expect("current exe");
    let output = Command::new(exe)
        .env(CASE, case)
        .env(DEPTH, depth.to_string())
        .env(STACK, kib.to_string())
        .arg("--exact")
        .arg("child")
        .output()
        .expect("run the child");
    let text = String::from_utf8_lossy(&output.stdout);
    // A filter that matches nothing exits 0 and would read as "it fits" for
    // every size - which is what the first version of this file measured.
    assert!(
        text.contains("1 passed") || !output.status.success(),
        "the child ran no test: {text}"
    );
    output.status.success()
}

fn fits(case: &str, kib: usize) -> bool {
    fits_at(case, kib, CAP)
}

/// The floor for one case at one nesting depth, in KiB.
fn floor_at(case: &str, depth: usize) -> Option<usize> {
    let mut smallest = None;
    for kib in [
        8192usize, 4096, 2048, 1536, 1280, 1024, 896, 768, 640, 512, 384, 320, 256, 192, 128, 96,
        64, 48, 32, 24, 16,
    ] {
        if fits_at(case, kib, depth) {
            smallest = Some(kib);
        } else {
            break;
        }
    }
    smallest
}

/// What one nesting level costs, derived rather than guessed.
#[test]
#[ignore = "a measurement, not an assertion"]
fn what_a_level_costs() {
    for depth in [25usize, 50, 100, 200] {
        match floor_at("parse", depth) {
            Some(kib) => println!("parse at depth {depth:>3}: fits in {kib}KiB"),
            None => println!("parse at depth {depth:>3}: needs more than 8192KiB"),
        }
    }
}

#[test]
fn child() {
    // Only does anything when the parent asked for a case; otherwise it is an
    // empty test the normal run passes over.
    let (Ok(case), Ok(kib)) = (std::env::var(CASE), std::env::var(STACK)) else {
        return;
    };
    run_case(case, kib.parse().expect("stack size"));
}

#[test]
#[ignore = "a measurement, not an assertion"]
fn where_the_stack_goes() {
    // CONTROL: the smallest probe must fail somewhere, or the harness is
    // measuring nothing at all.
    assert!(
        !fits("parse", 16),
        "a 16KiB stack parsed a 200-level document - the probe is not running the case"
    );

    for case in ["parse", "parse+drop", "parse+to_json", "to_html"] {
        let mut smallest = None;
        for kib in [
            8192usize, 4096, 2048, 1536, 1280, 1024, 960, 896, 832, 768, 704, 640, 576, 512, 384,
            256, 128, 64,
        ] {
            if fits(case, kib) {
                smallest = Some(kib);
            } else {
                break;
            }
        }
        match smallest {
            Some(kib) => println!("{case:<14} fits in {kib}KiB"),
            None => println!("{case:<14} needs more than 8192KiB"),
        }
    }
}

/// THE RATCHET, and the only assertion in this file.
///
/// Everything above is a measurement, deliberately `#[ignore]`d - which left
/// the floor itself unguarded. That is the shape carve-wasm#48 hit: a corpus
/// job flipped from green to red on an UNCHANGED commit and an identical
/// toolchain, because the margin was thin enough to move on its own and nothing
/// was watching it. A number nobody asserts is a number that drifts.
///
/// The ceilings are the measured floors plus headroom, per profile, because a
/// debug frame is not a release frame:
///
/// | profile | measured floor at the cap | ceiling here |
/// | --- | --- | --- |
/// | release | 384KiB | 512KiB |
/// | debug | 1024KiB | 1536KiB |
///
/// They are a RATCHET, not a target: when a change lowers the floor, lower
/// these with it, and the drop is what the commit is for. Raising one is a
/// regression and needs saying so out loud.
///
/// `parse+drop` is asserted beside `parse` because the AST frees through
/// compiler-generated recursive drop glue. Today it costs nothing extra - both
/// floor at the same number - so this pins the day that stops being true, which
/// is the day the parser gets cheap enough for teardown to become the binding
/// constraint (markup-carve/carve-rs#1165).
#[test]
fn the_floor_at_the_nesting_cap_does_not_regress() {
    let ceiling = if cfg!(debug_assertions) { 1536 } else { 512 };
    // CONTROL FIRST. A probe that runs no case reports "fits" for every size,
    // which is how the first version of this harness measured a 16KiB floor.
    assert!(
        !fits("parse", 16),
        "a 16KiB stack parsed a {CAP}-level document - the probe is not running the case"
    );
    for case in ["parse", "parse+drop"] {
        assert!(
            fits(case, ceiling),
            "{case} no longer fits a {ceiling}KiB stack at the {CAP}-level cap;              the parser's descent got more expensive, or the cap moved"
        );
    }
}
