//! PART 9 §25: at the render ceiling, a renderer refuses.
//!
//! Every renderer has a bound above the parser's, and what happens AT it was
//! unstated until carve#526 settled it: a typed, documented failure naming the
//! bound - the rule PART 12 §9(b) already applies to ingest, at the other end of
//! the same pipe. This engine returned empty output instead, so a caller got a
//! string that looked complete with its body deleted (carve-rs#511 item 5).
//! carve-js raises `RenderDepthError`, carve-php `RenderDepthExceededException`.
//!
//! The refusal is unreachable from a source string - the parse cap sits below
//! the ceiling - which is why `to_html` and its siblings still return `String`.
//! It is reachable for a tree built through the API or read by `from_json`,
//! which is the caller that can act on it, and that is what these tests build.

use std::collections::BTreeMap;

use carve::{
    BlockNode, BlockQuote, Document, InlineNode, Options, Paragraph, RenderCarveError,
    MAX_RENDER_DEPTH,
};

/// Deep trees are built, rendered and DROPPED here, and the recursive Drop
/// overflows a default test stack well before any renderer is reached - a
/// property of the tree type, not of the ceiling. The other deep-tree suites in
/// this crate spawn a big stack for the same reason.
fn on_big_stack<F: FnOnce() + Send + 'static>(f: F) {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(f)
        .expect("spawn worker")
        .join()
        .expect("worker must return, not abort");
}

/// A document whose only block is `depth` nested block quotes around a leaf.
///
/// Block quotes are the cheapest deep shape to build: one child each, and every
/// renderer descends them.
fn nested(depth: usize) -> Document {
    let mut block = BlockNode::Paragraph(Paragraph {
        attrs: None,
        children: vec![InlineNode::text("leaf".to_string())],
        ..Default::default()
    });
    for _ in 0..depth {
        block = BlockNode::BlockQuote(BlockQuote {
            fenced: false,
            attrs: None,
            children: vec![block],
            pos: None,
        });
    }
    Document {
        frontmatter: BTreeMap::new(),
        frontmatter_raw: None,
        source_len: 0,
        ingest_payload_len: 0,
        footnote_defs: BTreeMap::new(),
        footnote_def_pos: BTreeMap::new(),
        children: vec![block],
    }
}

fn render_all(doc: &Document) -> Vec<(&'static str, Result<String, RenderCarveError>)> {
    let options = Options::default();
    vec![
        (
            "html",
            carve::render_html_with_options(doc, &options).map_err(Into::into),
        ),
        (
            "markdown",
            carve::render_markdown_with_options(doc, &options).map_err(Into::into),
        ),
        (
            "plain",
            carve::render_plain_text_with_options(doc, &options).map_err(Into::into),
        ),
        (
            "ansi",
            carve::render_ansi_with_options(doc, &options).map_err(Into::into),
        ),
        ("carve", carve::render_carve(doc)),
    ]
}

#[test]
fn the_refusal_names_the_renderer_and_the_bound() {
    on_big_stack(|| {
        // The whole point of the typed error: a caller can tell WHICH target
        // stopped and at what bound, rather than inspecting output that looks
        // complete.
        let doc = nested(MAX_RENDER_DEPTH + 16);
        for (target, rendered) in render_all(&doc) {
            let err = rendered.expect_err("past the ceiling every target refuses");
            let RenderCarveError::Depth(err) = err else {
                panic!("the ceiling must return a depth refusal");
            };
            assert_eq!(err.renderer(), target);
            assert_eq!(err.limit(), MAX_RENDER_DEPTH);
            let shown = err.to_string();
            assert!(
                shown.contains(target),
                "message omits the renderer: {shown}"
            );
            assert!(
                shown.contains(&MAX_RENDER_DEPTH.to_string()),
                "message omits the bound: {shown}"
            );
        }
    });
}

#[test]
fn a_tree_within_the_ceiling_still_renders() {
    on_big_stack(|| {
        // The control. Without it the tests above would pass just as well if the
        // renderers had started refusing everything.
        let doc = nested(8);
        for (target, rendered) in render_all(&doc) {
            let output =
                rendered.unwrap_or_else(|err| panic!("{target} refused a shallow tree: {err}"));
            assert!(
                output.contains("leaf"),
                "{target} lost the body of a shallow tree: {output}"
            );
        }
    });
}

#[test]
fn a_list_at_the_parse_cap_renders_whole() {
    on_big_stack(|| {
        // The exact regression: a list ladder at the parse cap, through the
        // renderer that truncated it. carve-js and carve-php emit the whole
        // document here; this engine used to stop at the ceiling, because a
        // source level of a list costs two AST levels and the ceiling counted
        // the latter (markup-carve/carve#650).
        let source = (0..205)
            .map(|i| format!("{}- x", "  ".repeat(i)))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        let html = carve::render_html(&carve::parse(&source))
            .expect("a list at the parse cap is within the ceiling");
        // 200, not 205: the five levels past the PARSE cap degrade to literal
        // text, which is the parser's own visible degradation and not the
        // renderer's. What matters is that none of the 200 the parser built
        // went missing - it used to stop at about 120.
        assert_eq!(
            html.matches("<li>").count(),
            200,
            "the innermost items were dropped"
        );
    });
}

#[test]
fn the_source_path_stays_infallible() {
    on_big_stack(|| {
        // The parser caps nesting below the ceiling, so no source string can reach
        // it - which is what lets `to_*` keep returning `String`. A source far past
        // the cap degrades in the parser and renders without refusing.
        let source = format!("{}deep\n", "> ".repeat(MAX_RENDER_DEPTH + 64));
        let html = carve::to_html(&source);
        assert!(html.contains("deep"), "{html}");
        assert!(carve::to_markdown(&source).contains("deep"));
        assert!(carve::to_plain_text(&source).contains("deep"));
        assert!(carve::to_ansi(&source).contains("deep"));
        assert!(carve::to_carve(&source).contains("deep"));
    });
}

#[test]
fn a_nested_render_does_not_inherit_the_outer_refusal() {
    on_big_stack(|| {
        // The recorder is thread-local and installed per render, so a render that
        // refused must not make the NEXT one refuse. Without the RAII unwind, one
        // deep tree would poison every later render on the thread.
        let deep = nested(MAX_RENDER_DEPTH + 16);
        let _ = carve::render_html(&deep).expect_err("the deep tree refuses");

        let shallow = nested(2);
        let output = carve::render_html(&shallow).expect("a later render is unaffected");
        assert!(output.contains("leaf"), "{output}");
    });
}

/// §25 states the ceiling as a PROPERTY: a tree the same implementation's
/// parser produced must not be able to reach it. That is a claim about UNITS.
/// The parse cap counts source nesting levels and these renderers count AST
/// levels, so it was false here until the ceiling was restated as a factor: a
/// list spends two AST levels per source level, and about 120 nested items
/// truncated a document the parser had just accepted, where carve-js and
/// carve-php rendered it whole (markup-carve/carve#650).
#[test]
fn the_deepest_parsable_document_still_renders() {
    on_big_stack(|| {
        // Just past the parse cap: enough to exercise the parser's own
        // degradation, and no deeper - a ladder costs quadratic time to parse
        // in a debug build, which is what CI runs.
        let cap = 205;
        let shapes = [
            // The list is the shape the old ceiling actually truncated, and it
            // is deliberately shallower than the others: the canonical writer
            // is superlinear on nested lists (about 1ms at 40 levels and 259ms
            // at 200 in a release build, worse in the debug build CI runs),
            // which is its own defect and not this test's subject.
            (
                "list",
                (0..60)
                    .map(|i| format!("{}- x", "  ".repeat(i)))
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
            ("quote", format!("{}leaf", "> ".repeat(cap))),
            (
                "definition list",
                (0..cap / 2)
                    .map(|i| format!("{}:: t\n{}:  d", "  ".repeat(i), "  ".repeat(i)))
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
        ];
        for (name, source) in shapes {
            let source = format!("{source}\n");
            // Parsed ONCE and rendered five times, not parsed five times: a
            // ladder this deep is the slow part in a debug build, and the
            // property is about what the RENDERERS do with a tree the parser
            // produced.
            let doc = carve::parse(&source);
            for (target, rendered) in render_all(&doc) {
                let output = rendered.unwrap_or_else(|err| {
                    panic!("{target} refused a {name} the parser accepted: {err}")
                });
                assert!(
                    !output.trim().is_empty(),
                    "{target} emitted nothing for a {name} the parser accepted"
                );
            }
        }
    });
}
