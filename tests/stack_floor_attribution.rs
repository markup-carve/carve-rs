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
const CASE: &str = "CARVE_STACK_FLOOR_CASE";
const STACK: &str = "CARVE_STACK_FLOOR_KIB";

fn deep_source() -> String {
    format!(
        "{}deep\n{}",
        ":::: note\n".repeat(CAP),
        "::::\n".repeat(CAP)
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

fn fits(case: &str, kib: usize) -> bool {
    let exe = std::env::current_exe().expect("current exe");
    let output = Command::new(exe)
        .env(CASE, case)
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
