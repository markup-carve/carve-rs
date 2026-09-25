//! Deeply nested Markdown is REFUSED, at any depth, and never aborts the
//! process (carve-rs#1877).
//!
//! The Markdown importer is the only one whose tree has no nesting bound of its
//! own: the Carve parser caps its own, the HTML importer answers `DepthLimit`,
//! ingest has a JSON depth budget. So it was the only way untrusted input could
//! build a tree past PART 9 §25's ceiling - and the ceiling's typed refusal was
//! never reached, because the writer clones the tree before it walks it and
//! derived `Clone` has no ceiling to consult. The process died on SIGABRT
//! instead: no diagnostic, no exit code to branch on.
//!
//! WHY THE ASSERTIONS ARE ON THE ERROR AND NOT ON A DEPTH. The depth at which a
//! recursion overflows is a property of the machine that measured it - a debug
//! build overflows sooner than a release one, and a CI runner differs from both.
//! A test pinned to a threshold passes where it was written and fails elsewhere.
//! What is machine-independent is `MAX_RENDER_DEPTH`, and that every depth past
//! it answers the same way, so the depths below are written in terms of it and
//! the assertions are on the refusal.

use std::io::Write;
use std::process::{Command, Stdio};

use carve::MAX_RENDER_DEPTH;

/// Depths well past the ceiling, spanning three orders of magnitude.
///
/// 2,000 is the reported reproducer. The rest are there because the first fix
/// attempt only MOVED the cliff: bounding the writer's clone left the prepasses
/// recursing at 100,000, and bounding those left the tree's own `Drop` recursing
/// past that. A single depth cannot see any of it.
const PAST_THE_CEILING: [usize; 4] = [2_000, 10_000, 100_000, 1_000_000];

fn nested_quotes(depth: usize) -> String {
    "> ".repeat(depth) + "x\n"
}

/// Nested list items, the other shape that reaches an unbounded tree - one level
/// costs two frames there, where a quote costs one.
fn nested_items(depth: usize) -> String {
    let mut source = String::new();
    for level in 0..depth {
        source.push_str(&"  ".repeat(level));
        source.push_str("-\n");
    }
    source.push_str(&"  ".repeat(depth));
    source.push_str("- x\n");
    source
}

fn refusal(source: &str) -> carve::RenderCarveError {
    carve::try_migrate_markdown(source).expect_err("a tree past the ceiling must be refused")
}

fn assert_names_the_ceiling(error: &carve::RenderCarveError, at: &str) {
    match error {
        carve::RenderCarveError::Depth(depth) => {
            assert_eq!(depth.renderer(), "carve", "renderer at {at}");
            assert_eq!(depth.limit(), MAX_RENDER_DEPTH, "bound at {at}");
        }
        other => panic!("expected the depth refusal at {at}, got {other:?}"),
    }
}

#[test]
fn every_depth_past_the_ceiling_is_refused_with_the_same_error() {
    for depth in PAST_THE_CEILING {
        assert_names_the_ceiling(&refusal(&nested_quotes(depth)), &format!("{depth} quotes"));
    }
}

#[test]
fn nested_items_are_refused_too() {
    // Two frames per level, so the source grows with the square of the depth;
    // a few thousand levels is already far past the ceiling.
    let depth = 2 * MAX_RENDER_DEPTH + 100;
    assert_names_the_ceiling(&refusal(&nested_items(depth)), "nested items");
}

/// The control: the deepest tree the ceiling admits still migrates.
///
/// This is what pins the bound the importer now builds to as one that cannot
/// truncate a document any renderer would accept. `MAX_RENDER_DEPTH - 1` quotes
/// wrap a paragraph, which is exactly `MAX_RENDER_DEPTH` levels.
///
/// The ONLY test here that needs a bigger stack, and the reason is the one
/// `render_ceiling_refuses` gives: this case genuinely writes a tree 432 levels
/// deep, and the writer's recursion over it does not fit a 2 MiB test thread. It
/// costs that on main too. Every other test below runs on the default stack,
/// deliberately - a refusal that needed a bigger stack would not be a bound.
#[test]
fn the_deepest_admitted_tree_still_migrates() {
    let depth = MAX_RENDER_DEPTH - 1;
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            let migrated = carve::try_migrate_markdown(&nested_quotes(depth))
                .expect("the deepest admitted tree migrates");
            assert_eq!(migrated.value, nested_quotes(depth));
        })
        .expect("spawn worker")
        .join()
        .expect("worker must return, not abort");
}

/// One level further is refused, which is the behavior on both sides of the fix.
#[test]
fn one_level_past_the_admitted_tree_is_refused() {
    assert_names_the_ceiling(&refusal(&nested_quotes(MAX_RENDER_DEPTH)), "one level past");
}

/// Building the tree and DROPPING it is bounded on its own, with no render.
///
/// Derived `Drop` recurses, so a tree the importer was willing to build was one
/// the process could not free. A caller that only parses untrusted Markdown, and
/// never renders it, has to survive that.
#[test]
fn building_and_dropping_the_tree_is_bounded() {
    for depth in PAST_THE_CEILING {
        let doc = carve::markdown_to_ast(&nested_quotes(depth));
        assert_eq!(doc.children.len(), 1, "one root block at {depth}");
        drop(doc);
    }
}

/// Every tree-taking renderer refuses the same tree, rather than aborting in its
/// own prepass. `markdown` and `plain` and `ansi` looked bounded at 2,000 and
/// aborted at 100,000, in `crossref_index_for_document`.
#[test]
fn every_renderer_refuses_the_deep_tree() {
    let doc = carve::markdown_to_ast(&nested_quotes(*PAST_THE_CEILING.last().expect("depths")));
    assert!(carve::render_carve(&doc).is_err(), "carve");
    assert!(carve::render_html(&doc).is_err(), "html");
    assert!(carve::render_markdown(&doc).is_err(), "markdown");
    assert!(carve::render_plain_text(&doc).is_err(), "plain");
    assert!(carve::render_ansi(&doc).is_err(), "ansi");
}

/// The CLI answers with an exit CODE, which is the whole point.
///
/// `status.code()` is `None` for a process killed by a signal, so this
/// assertion is what tells a refusal apart from the SIGABRT it used to be -
/// stderr cannot, because an aborting process prints a message too.
#[test]
fn the_cli_exits_two_rather_than_dying() {
    for depth in PAST_THE_CEILING {
        let mut child = Command::new(env!("CARGO_BIN_EXE_carve"))
            .args(["migrate", "--from", "markdown"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn carve binary");
        let mut stdin = child.stdin.take().expect("stdin");
        match stdin.write_all(nested_quotes(depth).as_bytes()) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => {}
            Err(error) => panic!("write stdin: {error}"),
        }
        drop(stdin);
        let out = child.wait_with_output().expect("wait carve binary");
        assert_eq!(
            out.status.code(),
            Some(2),
            "at {depth}: stderr was {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.stdout.is_empty(), "no partial document at {depth}");
    }
}
