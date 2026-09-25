//! `carve fmt` deleted a directive's quoted title from the author's own file
//! (carve-rs#1880). The parser read the opener's title and threw it away, so the
//! writer had nothing left to put back.
//!
//! The formatter's own invariants cannot witness that. `parse(fmt(x)) ==
//! parse(x)` held, because the title was absent from BOTH trees, and idempotence
//! held because the second pass deleted nothing more. So every assertion here
//! reads the SOURCE.

use carve::ast::{BlockNode, Document, GENERATED_CONTENT_KINDS};

fn parse(src: &str) -> Document {
    carve::parse(src)
}

fn only_directive(doc: &Document) -> &carve::ast::Directive {
    assert_eq!(doc.children.len(), 1, "one block: {:?}", doc.children);
    match &doc.children[0] {
        BlockNode::Directive(d) => d,
        other => panic!("expected a directive, got {other:?}"),
    }
}

#[test]
fn the_ticket_repro_keeps_both_tokens() {
    let src = "::: toc \"Contents\" [T]\n:::\n";
    let formatted = carve::to_carve(src);
    assert!(
        formatted.contains("\"Contents\""),
        "the title is authored text: {formatted:?}"
    );
    assert!(
        formatted.contains("[T]"),
        "the label survived even before the fix: {formatted:?}"
    );
}

#[test]
fn every_generated_content_kind_keeps_its_title() {
    for kind in GENERATED_CONTENT_KINDS {
        let src = format!("::: {kind} \"Heading\"\n:::\n");
        let formatted = carve::to_carve(&src);
        assert!(
            formatted.contains("\"Heading\""),
            "{kind} dropped its title: {formatted:?}"
        );
        assert!(
            matches!(parse(&formatted).children[0], BlockNode::Directive(_)),
            "{kind} still re-parses to a directive: {formatted:?}"
        );
    }
}

#[test]
fn the_title_reaches_the_node_as_inline_content() {
    // Not a string: the same shape `admonition.title` carries, so `*Notes*`
    // round-trips as emphasis rather than as three literal characters.
    let doc = parse("::: footnotes \"*Notes*\"\n:::\n");
    let title = only_directive(&doc)
        .title
        .as_ref()
        .expect("the opener spelled one");
    let [carve::ast::InlineNode::Emphasis(strong)] = title.as_slice() else {
        panic!("inline nodes, not text: {title:?}");
    };
    assert_eq!(strong.kind, carve::ast::EmphasisKind::Strong);
    assert!(
        carve::to_carve("::: footnotes \"*Notes*\"\n:::\n").contains("\"*Notes*\""),
        "and the writer spells the emphasis back"
    );
}

#[test]
fn a_quote_inside_the_title_stays_escaped() {
    let src = "::: index \"A \\\"quoted\\\" word\"\n:::\n";
    let formatted = carve::to_carve(src);
    assert!(
        formatted.contains("\\\"quoted\\\""),
        "an unescaped quote would close the title early: {formatted:?}"
    );
    assert_eq!(
        parse(&formatted).children[0],
        parse(src).children[0],
        "and the re-parse is the same node"
    );
}

#[test]
fn formatting_is_idempotent_on_a_titled_directive() {
    let src = "::: references \"Sources\" [S]\n:::\n";
    let once = carve::to_carve(src);
    assert_eq!(carve::to_carve(&once), once, "fmt(fmt(x)) == fmt(x)");
}

#[test]
fn an_untitled_directive_formats_exactly_as_before() {
    // The fix must not reach a marker that spells no title, or the corpus
    // formatter sweep moves.
    for src in [
        "::: toc\n:::\n",
        "::: footnotes [End]\n:::\n",
        "{.wide}\n::: glossary\n:::\n",
    ] {
        let formatted = carve::to_carve(src);
        assert!(
            !formatted.contains('"'),
            "no title token appears from nowhere: {formatted:?}"
        );
    }
}

#[test]
fn the_wire_does_not_carry_it_at_this_spec_pin() {
    // The schema this pin names closes `directive` without a `title`, and
    // `WIRE_FIELDS` is generated from it, so publishing the field here would
    // emit a property ingest refuses. Moving the pin and the wire is
    // carve-rs#1874.
    let json = carve::to_json(&parse("::: toc \"Contents\"\n:::\n"));
    assert!(
        json.contains(r#""type":"directive""#),
        "still a directive: {json}"
    );
    assert!(
        !json.contains("Contents"),
        "the wire has no slot for it yet: {json}"
    );
}

#[test]
fn a_profile_reaches_the_title_before_the_writer_does() {
    // The writer emits the title, so a profile that denies a node inside it has
    // to filter there too, or the denied node rides back out into the formatted
    // source. `minimal` degrades a link to its text.
    // The body is load-bearing: a childless container is pruned outright once any
    // profile is set, whatever it denies, so the title would go with the node and
    // this would pass without the filter ever reaching a title (carve-rs#1897).
    let src = "::: toc \"See [text](https://example.com)\"\nbody\n:::\n";
    // Not `minimal`, which denies `directive` itself and degrades the whole node.
    let options = carve::Options {
        profile: Some(carve::Profile::default().deny_inline(&["link"])),
        ..Default::default()
    };
    let filtered = carve::to_carve_with_options(src, &options);
    assert!(
        !filtered.contains("https://example.com"),
        "the profile did not reach the title: {filtered:?}"
    );
    assert!(
        filtered.contains("See text"),
        "the degraded form keeps the link text: {filtered:?}"
    );

    // The pair, so the assertion above is about the profile and not about the
    // writer having stopped emitting titles.
    let unfiltered = carve::to_carve(src);
    assert!(
        unfiltered.contains("https://example.com"),
        "without a profile the link survives: {unfiltered:?}"
    );
}

#[test]
fn the_title_inlines_carry_offsets_that_slice_back() {
    let src = "::: bibliography \"Works Cited\"\n:::\n";
    let doc = carve::parse_with_options(
        src,
        &carve::Options {
            positions: true,
            ..Default::default()
        },
    );
    let title = only_directive(&doc)
        .title
        .as_ref()
        .expect("the opener spelled one");
    let carve::ast::InlineNode::Text(text) = &title[0] else {
        panic!("one text run: {title:?}");
    };
    let pos = text.pos.as_ref().expect("a positioned text run");
    assert_eq!(
        &src[pos.start_offset..pos.end_offset],
        "Works Cited",
        "an offset pair left at 0..0 reads as present and selects nothing"
    );
}

#[test]
fn a_title_past_the_render_ceiling_is_refused_not_walked() {
    // The writer renders the title, so the depth precheck has to count it. A
    // 32 MiB stack because a deeply nested AST costs one debug frame per level
    // (carve-rs#530).
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(|| {
            use carve::ast::{BlockNode, Directive, Emphasis, EmphasisKind, InlineNode, Text};

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
            let mut doc = carve::parse("x\n");
            doc.children = vec![BlockNode::Directive(Directive {
                attrs: None,
                kind: "toc".to_string(),
                title: Some(vec![title]),
                label: None,
                children: Vec::new(),
                pos: None,
            })];

            let err = carve::render_carve(&doc).expect_err("past the ceiling the writer refuses");
            let carve::RenderCarveError::Depth(err) = err else {
                panic!("the ceiling must return a depth refusal");
            };
            assert_eq!(err.renderer(), "carve");
            assert_eq!(err.limit(), carve::MAX_RENDER_DEPTH);
        })
        .expect("thread spawns")
        .join()
        .expect("the check finishes");
}
