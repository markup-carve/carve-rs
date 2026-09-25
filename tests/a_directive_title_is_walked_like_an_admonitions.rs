//! A `directive` carries the opener's quoted title as of carve-rs#1895, and the
//! title renders on every target. Three traversals still walked only the node's
//! CHILDREN, so each of them saw a document the renderers do not render.
//!
//! Every assertion pairs the directive with the same shape on an ADMONITION,
//! whose title those traversals already visit. Without the pair a green run
//! could mean the pass had stopped doing its job for both.

use carve::ast::{BlockNode, InlineNode};

fn deny_link() -> carve::Options<'static> {
    carve::Options {
        profile: Some(carve::Profile::default().deny_inline(&["link"])),
        ..Default::default()
    }
}

const TITLED_LINK: &str = "See [text](https://example.com)";

#[test]
fn a_profile_reaches_the_title_on_every_target() {
    let directive = format!("::: toc \"{TITLED_LINK}\"\nbody\n:::\n");
    let admonition = format!("::: note \"{TITLED_LINK}\"\nbody\n:::\n");
    let options = deny_link();

    for (what, src) in [("directive", &directive), ("admonition", &admonition)] {
        let html = carve::to_html_with_options(src, &options);
        assert!(
            !html.contains("https://example.com"),
            "{what}: a denied link reached the HTML: {html}"
        );
        let formatted = carve::to_carve_with_options(src, &options);
        assert!(
            !formatted.contains("https://example.com"),
            "{what}: a denied link rode back into the formatted source: {formatted:?}"
        );
    }

    // The pair that makes the two assertions above about the PROFILE: without
    // one the link survives, so they are not passing because the title stopped
    // being rendered.
    assert!(carve::to_html(&directive).contains("https://example.com"));
    assert!(carve::to_carve(&directive).contains("https://example.com"));
}

#[test]
fn the_title_inlines_carry_byte_offsets() {
    // Line and column were filled while the offsets stayed 0..0, which reads as
    // a present span that selects nothing.
    let options = carve::Options {
        positions: true,
        ..Default::default()
    };
    for src in [
        "::: bibliography \"Works Cited\"\n:::\n",
        "::: note \"Works Cited\"\n:::\n",
    ] {
        let doc = carve::parse_with_options(src, &options);
        let title = match &doc.children[0] {
            BlockNode::Directive(d) => d.title.as_ref(),
            BlockNode::Admonition(a) => a.title.as_ref(),
            other => panic!("unexpected block: {other:?}"),
        }
        .expect("the opener spelled a title");
        let InlineNode::Text(text) = &title[0] else {
            panic!("one text run: {title:?}");
        };
        let pos = text.pos.as_ref().expect("a positioned run");
        assert_eq!(
            &src[pos.start_offset..pos.end_offset],
            "Works Cited",
            "{src:?} sliced back to something else"
        );
    }
}

#[test]
fn a_title_past_the_render_ceiling_is_refused_not_walked() {
    // The writer renders the title, so the depth precheck has to count it or the
    // typed refusal is bypassed and the recursive clone below it runs. 32 MiB
    // because a deeply nested AST costs one debug frame per level (carve-rs#530).
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(|| {
            use carve::ast::{Admonition, Directive, Emphasis, EmphasisKind, Text};

            let deep_title = || {
                let mut title = InlineNode::Text(Text {
                    value: "deep".to_string(),
                    pos: None,
                });
                for _ in 0..carve::MAX_RENDER_DEPTH + 1 {
                    title = InlineNode::Emphasis(Emphasis {
                        attrs: None,
                        kind: EmphasisKind::Italic,
                        children: vec![title],
                        pos: None,
                    });
                }
                Some(vec![title])
            };
            let blocks = [
                BlockNode::Directive(Directive {
                    attrs: None,
                    kind: "toc".to_string(),
                    title: deep_title(),
                    label: None,
                    children: Vec::new(),
                    pos: None,
                }),
                BlockNode::Admonition(Admonition {
                    attrs: None,
                    kind: "note".to_string(),
                    title: deep_title(),
                    label: None,
                    children: Vec::new(),
                    pos: None,
                }),
            ];
            for block in blocks {
                let mut doc = carve::parse("x\n");
                doc.children = vec![block];
                let err =
                    carve::render_carve(&doc).expect_err("past the ceiling the writer refuses");
                let carve::RenderCarveError::Depth(err) = err else {
                    panic!("the ceiling must return a depth refusal");
                };
                assert_eq!(err.renderer(), "carve");
                assert_eq!(err.limit(), carve::MAX_RENDER_DEPTH);
            }
        })
        .expect("thread spawns")
        .join()
        .expect("the check finishes");
}
