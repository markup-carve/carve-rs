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
const SHAPE: &str = "CARVE_STACK_FLOOR_SHAPE";

/// The nesting SHAPE the ladder is built from.
///
/// One axis this file did not have. Every number in it was measured on a colon
/// container, and markup-carve/carve-rs#1165 is explicit that the shapes do not
/// cost the same: at the point it was filed containers needed 768KiB, block
/// quotes ~768KiB and lists ~1024KiB, and after markup-carve/carve-rs#1177
/// halved the frames they were 384 / 256 / 768. So a floor asserted on
/// containers alone says nothing about the shape that was ALWAYS the tallest,
/// and `parse_list` - the one the ticket parks as a restructure rather than an
/// extraction - was the shape nothing watched.
///
/// THE INLINE AXIS IS NOT HERE, and that is a measurement rather than an
/// omission. markup-carve/carve-rs#1165 names inline nesting as the second axis,
/// but a braced-span ladder cannot reach a depth that would matter:
/// `parse_forced_emphasis` closes on the FIRST `delim}` pair after its opener,
/// so a repeated `{*` collapses at once, and a ladder cycling all five
/// delimiters reaches SIX levels and then collapses the same way. Measured at
/// depths 5, 25 and 200: the parsed inline depth is 6, 7 and 7. A probe over a
/// shape that does not nest is a probe that measures nothing, which is the
/// failure this file's control exists to catch, so it is not shipped as one.
fn ladder(depth: usize) -> String {
    match std::env::var(SHAPE).unwrap_or_default().as_str() {
        // A block quote nests on its MARKER RUN, so depth is columns of `> `
        // on one line rather than a stack of openers.
        "quote" => format!("{}deep\n", "> ".repeat(depth)),
        // Two spaces a level, which is what `list_indent_model_a` reads as a
        // sub-list. `parse_list` owns the item loop and stays on the stack
        // while the parser descends into an item's content, which is the whole
        // reason extraction did not move this number.
        "list" => {
            let mut out = String::new();
            for level in 0..depth {
                out.push_str(&"  ".repeat(level));
                out.push_str("- x\n");
            }
            out
        }
        // The default, and every number this file measured before the axis
        // existed.
        _ => format!(
            "{}deep\n{}",
            ":::: note\n".repeat(depth),
            "::::\n".repeat(depth)
        ),
    }
}

fn deep_source() -> String {
    let depth: usize = std::env::var(DEPTH)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(CAP);
    ladder(depth)
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
        // A SECOND descent, not a variation on the first: positions on takes
        // the mapped-source chain, which carries the container's line and
        // column maps and never reaches the unpositioned helpers. Nothing here
        // measured it, so a floor could move on that side unwatched - which is
        // the gap this file exists to close.
        "parse+positions" => {
            let doc =
                carve::parse_with_options(source, &carve::Options::default().with_positions(true));
            std::mem::forget(doc);
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
        // `to_html` over a SOURCE gets the typed layout fast path, which is a
        // different descent from the one `parse` takes. This case pays for the
        // ordinary parse and then renders the tree, so the two numbers separate
        // the fast path from the walk it replaces.
        "parse+render" => {
            let doc = carve::parse(source);
            let html = carve::render_html(&doc).expect("renders");
            assert!(!html.is_empty());
            std::mem::forget(doc);
        }
        other => panic!("unknown case {other}"),
    }
    true
}

fn fits_at(case: &str, kib: usize, depth: usize) -> bool {
    fits_shaped(case, kib, depth, "container")
}

fn fits_shaped(case: &str, kib: usize, depth: usize, shape: &str) -> bool {
    let exe = std::env::current_exe().expect("current exe");
    let output = Command::new(exe)
        .env(CASE, case)
        .env(DEPTH, depth.to_string())
        .env(SHAPE, shape)
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

/// What each nesting SHAPE costs, which the container ladder alone cannot say.
#[test]
#[ignore = "a measurement, not an assertion"]
fn where_the_stack_goes_per_shape() {
    for shape in ["container", "quote", "list"] {
        for case in ["parse", "to_html", "parse+render"] {
            let mut smallest = None;
            for kib in [
                8192usize, 4096, 2048, 1536, 1280, 1024, 896, 768, 640, 512, 384, 320, 256, 192,
                128, 96, 64, 48, 32,
            ] {
                if fits_shaped(case, kib, CAP, shape) {
                    smallest = Some(kib);
                } else {
                    break;
                }
            }
            match smallest {
                Some(kib) => println!("{shape:<10} {case:<8} fits in {kib}KiB"),
                None => println!("{shape:<10} {case:<8} needs more than 8192KiB"),
            }
        }
    }
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

    for case in [
        "parse",
        "parse+drop",
        "parse+positions",
        "parse+to_json",
        "to_html",
    ] {
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

/// THE SECOND RATCHET: one per nesting SHAPE, because the shapes do not cost
/// the same and only one of them was ever watched.
///
/// Measured at the 200-level cap, in child processes, on this commit:
///
/// | shape | `parse` rel | ceiling | `parse` dbg | ceiling |
/// | --- | --- | --- | --- | --- |
/// | container | 64KiB | 96KiB | 256KiB | 512KiB |
/// | quote | 320KiB | 512KiB | 4096KiB | 6144KiB |
/// | list | 768KiB | 1024KiB | 8192KiB | 12288KiB |
///
/// LISTS ARE THE TALLEST BY A FACTOR OF SIX, and until now nothing asserted
/// them at all: every number in this file was a colon container, which
/// markup-carve/carve-rs#1185 made cheap. `parse_list` owns the item loop and
/// stays on the stack while the parser descends into an item's content, so the
/// three extraction attempts recorded on markup-carve/carve-rs#1165 could not
/// move it - one of them made the machine pay MORE, because the extracted
/// branch brought its own frame and both were live on the recursive path. What
/// remains there is a restructure, and it is parked with its own measurements.
///
/// This is the guard that has to exist either way: 768KiB release / 8192KiB
/// debug is the number that restructure would be judged against, and a number
/// nobody asserts is a number that drifts - which is exactly the shape
/// carve-wasm#48 hit on an unchanged commit.
#[test]
fn the_floor_for_every_nesting_shape_does_not_regress() {
    let debug = cfg!(debug_assertions);
    // CONTROL FIRST, per shape. A probe that runs no case reports "fits" for
    // every size, which is how the first version of this harness measured a
    // 16KiB floor.
    for shape in ["container", "quote", "list"] {
        assert!(
            !fits_shaped("parse", 16, CAP, shape),
            "a 16KiB stack parsed a {CAP}-level {shape} - the probe is not running the case"
        );
    }
    for (shape, release_ceiling, debug_ceiling) in [
        ("container", 96, 512),
        ("quote", 512, 6144),
        ("list", 1024, 12288),
    ] {
        let ceiling = if debug {
            debug_ceiling
        } else {
            release_ceiling
        };
        assert!(
            fits_shaped("parse", ceiling, CAP, shape),
            "a {CAP}-level {shape} no longer fits a {ceiling}KiB stack; that shape's \
             descent got more expensive, or the cap moved"
        );
    }
}

/// `to_html` OVER A SOURCE DOES NOT DESCEND THE WAY `parse` DOES, and on the
/// tallest shape that is an EIGHTFOLD difference rather than a detail.
///
/// Measured, release, 200-level list: `to_html` fits 96KiB where
/// `parse` and `parse+render` both need 768KiB. The typed layout fast path
/// answers that document without `parse_list`, so the walk that sets this
/// engine's stack floor is reachable through `parse` / `parseJson` and NOT
/// through `toHtml`. Worth asserting because it decides which entry point a
/// wasm host has to worry about - carve-wasm#48 crashed on both, and only one
/// of them is still the expensive one.
#[test]
fn the_html_fast_path_is_cheaper_than_the_parse_it_replaces() {
    let cheap = if cfg!(debug_assertions) { 384 } else { 128 };
    assert!(
        fits_shaped("to_html", cheap, CAP, "list"),
        "to_html over a {CAP}-level list no longer fits {cheap}KiB - the typed layout \
         fast path stopped answering it, and the floor is now `parse`'s"
    );
    // THE PAIR, or the assertion above would also pass on a build where every
    // path had become cheap - which would be good news, and would still mean
    // this test had stopped measuring the difference it is named for.
    assert!(
        !fits_shaped("parse+render", cheap, CAP, "list"),
        "parsing a {CAP}-level list now fits {cheap}KiB too, so there is no fast-path \
         difference left to assert: re-measure and rewrite this test"
    );
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
/// debug frame is not a release frame - and in TWO groups, because parsing and
/// rendering no longer descend the same amount:
///
/// | case | release floor | ceiling | debug floor | ceiling |
/// | --- | --- | --- | --- | --- |
/// | `parse`, `parse+drop`, `parse+to_json` | 64KiB | 96KiB | 256KiB | 512KiB |
/// | `parse+positions` | 128KiB | 192KiB | 384KiB | 512KiB |
/// | `to_html` | 256KiB | 384KiB | 640KiB | 768KiB |
///
/// They are a RATCHET, not a target: when a change lowers the floor, lower
/// these with it, and the drop is what the commit is for. Raising one is a
/// regression and needs saying so out loud.
///
/// The parse numbers came down from 384KiB release / 1024KiB debug when the
/// colon-container descent became a worklist (markup-carve/carve-rs#1165): the
/// nesting the cap admits 200 of now costs heap rather than host stack. They
/// HALVED again, 128KiB to 64KiB release, when the two post-parse walks that run
/// on EVERY parse - `collect_explicit_ids` and `collect_heading_titles` - became
/// worklists too (markup-carve/carve-rs#1186). That drop IS the attribution:
/// those two owned half of what a level still cost, on a ladder with no heading
/// and no caption in it, which they walked all 200 levels of anyway.
///
/// What is left growing with depth is the positions chain (`fill_offsets`,
/// `apply_inline_offsets`) and, for `to_html`, the renderer's own descent -
/// which is why that case still floors ABOVE the parse it renders.
///
/// `parse+drop` is asserted beside `parse` because the AST frees through
/// compiler-generated recursive drop glue. It STILL costs nothing extra - both
/// floor at the same number - so the day this pins has not arrived: the drop
/// glue only becomes the binding constraint once the walks above it are cheaper
/// than it is.
#[test]
fn the_floor_at_the_nesting_cap_does_not_regress() {
    let debug = cfg!(debug_assertions);
    // CONTROL FIRST. A probe that runs no case reports "fits" for every size,
    // which is how the first version of this harness measured a 16KiB floor.
    assert!(
        !fits("parse", 16),
        "a 16KiB stack parsed a {CAP}-level document - the probe is not running the case"
    );
    // TWO GROUPS ON THE PARSE SIDE NOW, because the positions chain is a
    // SECOND descent and it did not come down with the first
    // (carve-rs#1186). `fill_offsets` and `apply_inline_offsets` still walk a
    // container's children, and `fill_offsets` reads an item's children AFTER
    // recursing into them, so it wants the put-something-back worklist shape
    // rather than the plain one - it is named as remaining work, not converted
    // here. Keeping it under the same ceiling as `parse` would have hidden the
    // halving below.
    let parse_ceiling = if debug { 512 } else { 96 };
    for case in ["parse", "parse+drop", "parse+to_json"] {
        assert!(
            fits(case, parse_ceiling),
            "{case} no longer fits a {parse_ceiling}KiB stack at the {CAP}-level cap; \
             the parser's descent got more expensive, or the cap moved"
        );
    }
    let positions_ceiling = if debug { 512 } else { 192 };
    assert!(
        fits("parse+positions", positions_ceiling),
        "parse+positions no longer fits a {positions_ceiling}KiB stack at the {CAP}-level \
         cap; the positions chain's descent got more expensive, or the cap moved"
    );
    // The RENDER walk, which the parse cases cannot see. It is the taller of
    // the two now, and it is one of the two entry points carve-wasm#48 crashed.
    let render_ceiling = if debug { 768 } else { 384 };
    assert!(
        fits("to_html", render_ceiling),
        "to_html no longer fits a {render_ceiling}KiB stack at the {CAP}-level cap; \
         the renderer's descent got more expensive, or the cap moved"
    );
}
