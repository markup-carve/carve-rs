//! `carve --carve` aborted with exit 101 on a list one level past the nesting
//! cap whose deepest item holds a code fence (carve-rs#2326). `to_carve`
//! unwrapped the writer's refusal with an `expect` saying a parsed tree never
//! reaches the render ceiling. The DEPTH half of that claim is true; a
//! ROUND-TRIP refusal is not, and past the cap the over-cap flattening hands
//! the writer a code node whose value carries the ladder's indentation, which
//! the block layer strips on the way back out.
//!
//! The threshold is exactly the cap, so the shallower ladder beside each case
//! is the control: a refusal that fired one level early would pass every
//! assertion about the deep document and fail the shallow one.

use carve::{try_to_carve_with_options, CarveWriteError, Options};

/// A worst-case-depth ladder needs more than the 2 MiB a test thread gets in a
/// debug build; the library is fine with it, as the CLI on the main thread
/// shows. Same convention as `over_cap_paragraph_position.rs`.
fn on_big_stack<F: FnOnce() + Send + 'static>(f: F) {
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(f)
        .expect("spawn worker")
        .join()
        .expect("worker must return, not abort");
}

/// A ladder of `depth` bullets whose deepest item holds a fenced code block.
fn ladder(depth: usize) -> String {
    let mut lines: Vec<String> = (0..depth)
        .map(|level| format!("{}- x", "  ".repeat(level)))
        .collect();
    let pad = "  ".repeat(depth);
    lines.push(format!("{pad}```"));
    lines.push(format!("{pad}code"));
    lines.push(format!("{pad}```"));
    lines.join("\n") + "\n"
}

#[test]
fn one_level_past_the_cap_is_a_refusal_and_not_an_abort() {
    on_big_stack(|| {
        let error = try_to_carve_with_options(&ladder(201), &Options::default())
            .expect_err("the writer cannot spell the flattened code node");
        let CarveWriteError::Render(error) = error else {
            panic!("a default `Options` carries no profile, so this is the writer's own refusal");
        };
        assert_eq!(
            error.to_string(),
            "the Carve renderer cannot spell code: a line of the value starts with whitespace, \
                 which the block layer strips"
        );

        // AND THE INFALLIBLE ENTRY POINT LEAVES THE SOURCE AS AUTHORED. It has
        // no way to report the refusal, and every caller formats in place, so
        // returning the text unchanged loses nothing where aborting or emitting
        // an empty document would. Asserted on the same ladder: a second
        // over-cap parse costs the suite as much again.
        let source = ladder(201);
        assert_eq!(carve::to_carve(&source), source);
    });
}

/// AT the cap the document still writes, which is what makes the case above a
/// refusal at the right depth rather than a refusal at any depth.
#[test]
fn at_the_cap_the_document_still_writes() {
    on_big_stack(|| {
        let written = try_to_carve_with_options(&ladder(200), &Options::default())
            .expect("at the cap the writer spells the tree");
        assert!(
            written.starts_with("- x\n"),
            "got {:?}",
            &written[..40.min(written.len())]
        );
    });
}

/// AND IT STILL FORMATS EVERYTHING ELSE, so the arm above cannot be reached by
/// a document the writer can spell.
#[test]
fn the_infallible_entry_point_still_formats() {
    assert_eq!(carve::to_carve("-   a\n"), "- a\n");
}
