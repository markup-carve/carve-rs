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

// ---------------------------------------------------------------------------
// carve-rs#1919: the same divergence at ten more call sites.
//
// Each case renders the SAME title on `::: toc` and on `::: note` and asserts
// the two answer alike. The admonition is the control: it is the arm that
// already visits the title, so a case passing for both is the pass still
// working, and a case passing for neither would be a pass that stopped.
// ---------------------------------------------------------------------------

/// The title and its container kind, for a case that differs only in the kind.
fn titled(kind: &str, title: &str) -> String {
    format!("::: {kind} \"{title}\"\n:::\n")
}

#[test]
fn an_id_declared_in_the_title_is_reserved() {
    // Unreserved, a later heading mints the same id and the document carries
    // two `id="dup"` - invalid HTML, not a cosmetic divergence.
    for kind in ["toc", "note"] {
        let html = carve::to_html(&format!("::: {kind} \"A[x]{{#dup}}\"\n:::\n\n# dup\n"));
        assert!(
            html.contains("<section id=\"dup-2\">"),
            "{kind}: the heading reused the title's id: {html}"
        );
    }
}

#[test]
fn a_crossref_in_the_title_keeps_the_target_heading_anchored() {
    // The Markdown writer emits `{#id}` only for a heading something references.
    // A reference the prepass cannot see leaves the link pointing at nothing.
    for kind in ["toc", "note"] {
        let md = carve::to_markdown(&format!("::: {kind} \"See </#H>\"\n:::\n\n# H\n"));
        assert!(md.contains("[H](#H)"), "{kind}: no link was written: {md}");
        assert!(
            md.contains("# H {#H}"),
            "{kind}: the referenced heading lost its anchor: {md}"
        );
    }
}

#[test]
fn a_lint_rule_reaches_the_title() {
    let opts =
        carve::Options::new().with_extension(&carve::extensions::semantic_span::SemanticSpan);
    for kind in ["toc", "note"] {
        let src = titled(kind, "A `c`{kbd=V}");
        let warnings = carve::lint_carve_with_options(&src, &opts);
        assert_eq!(
            warnings.len(),
            1,
            "{kind}: expected the one attribute warning, got {warnings:?}"
        );
        assert_eq!(warnings[0].rule, "semantic-attribute-outside-span");
        // The rule describes what the renderer emits, so the attribute really
        // is there to describe.
        assert!(carve::to_html_with_options(&src, &opts).contains("kbd=\"V\""));
    }
}

#[test]
fn a_citation_in_the_title_is_annotated() {
    let ext = carve::Citations::new();
    let opts = carve::Options::new().with_extension(&ext);
    for kind in ["toc", "note"] {
        let html = carve::to_html_with_options(
            &format!(
                "::: {kind} \"As [@smith] shows\"\n:::\n\n[@smith]: {{author=Smith year=2020}} A Book\n"
            ),
            &opts,
        );
        assert!(
            html.contains("href=\"#ref-smith\""),
            "{kind}: the citation stayed literal: {html}"
        );
        // The pool is built from the annotated uses, so an unannotated title
        // loses the references list along with the link.
        assert!(
            html.contains("<ol class=\"references\">"),
            "{kind}: no references list was generated: {html}"
        );
    }
}

#[test]
fn a_flattened_ruby_in_the_title_is_reported() {
    // Carve 0.1 spells no ruby, so this shape arrives over the AST-JSON wire.
    // Unreported it is a silent loss: `--strict-losses` would accept it.
    for (ty, kind) in [("directive", "toc"), ("admonition", "note")] {
        let wire = format!(
            "{{\"type\":\"document\",\"srcByteLength\":0,\"children\":[{{\"type\":\"{ty}\",\
             \"kind\":\"{kind}\",\"title\":[{{\"type\":\"ruby\",\"pairs\":[{{\"base\":\
             [{{\"type\":\"text\",\"value\":\"kanji\"}}],\"annotation\":[{{\"type\":\"text\",\
             \"value\":\"ruby\"}}]}}]}}],\"children\":[]}}]}}"
        );
        let doc = carve::from_json(&wire).expect("the wire document decodes");
        let report = carve::with_render_loss_report(
            carve::RenderTarget::Carve,
            carve::CheckedRenderOptions::default(),
            || carve::render_carve(&doc).expect("the writer writes"),
        )
        .expect("the render reports");
        assert_eq!(
            report
                .losses
                .iter()
                .filter(|l| l.code == "ruby-flattened")
                .count(),
            1,
            "{ty}: the flattened ruby went unreported: {:?}",
            report.losses
        );
    }
}

#[test]
fn locale_quotes_reach_the_title() {
    let ext = carve::SmartQuotes::new("de");
    let opts = carve::Options::new().with_extension(&ext);
    for kind in ["toc", "note"] {
        let html = carve::to_html_with_options(&titled(kind, "it 'x' y"), &opts);
        assert!(
            html.contains("it \u{201a}x\u{2018} y"),
            "{kind}: the title kept the English glyphs: {html}"
        );
    }
}

#[test]
fn an_index_marker_in_the_title_is_counted() {
    let ext = carve::Index::new();
    let opts = carve::Options::new().with_extension(&ext);
    for kind in ["toc", "note"] {
        let html = carve::to_html_with_options(
            &format!("::: {kind} \"A :index[parser] here\"\n:::\n\n::: index\n:::\n"),
            &opts,
        );
        assert!(
            html.contains("id=\"idx-parser-1\""),
            "{kind}: the marker was not counted: {html}"
        );
        assert!(
            html.contains("<a href=\"#idx-parser-1\" class=\"index-backref\""),
            "{kind}: the term never reached the index list: {html}"
        );
    }
}

#[test]
fn a_numbered_crossref_in_the_title_is_rewritten() {
    let ext = carve::HeadingNumbers::new();
    let opts = carve::Options::new().with_extension(&ext);
    for kind in ["toc", "note"] {
        let html = carve::to_html_with_options(
            &format!("# Parsing\n\n::: {kind} \"See </#Parsing>\"\n:::\n"),
            &opts,
        );
        assert!(
            html.contains(">Section 1 - Parsing</a>"),
            "{kind}: the reference kept its unnumbered label: {html}"
        );
    }
}

#[test]
fn the_external_link_policy_reaches_the_title() {
    // A policy boundary, like the profile filter above: without it the title is
    // the one position where a third-party link keeps `window.opener`.
    let ext = carve::ExternalLinks::new();
    let opts = carve::Options::new().with_extension(&ext);
    for kind in ["toc", "note"] {
        let html =
            carve::to_html_with_options(&titled(kind, "See [docs](https://example.com)"), &opts);
        assert!(
            html.contains("rel=\"noopener noreferrer\""),
            "{kind}: the policy did not reach the title: {html}"
        );
    }
}

#[test]
fn tabs_in_the_titles_code_are_normalized() {
    let ext = carve::TabNormalize::new();
    let opts = carve::Options::new().with_extension(&ext);
    for kind in ["toc", "note"] {
        let html = carve::to_html_with_options(&titled(kind, "A `x\ty` z"), &opts);
        assert!(
            html.contains("<code>x  y</code>"),
            "{kind}: the tab survived: {html}"
        );
        assert!(
            !html.contains('\t'),
            "{kind}: a tab reached the HTML: {html}"
        );
    }
}

#[test]
fn a_same_kind_nesting_in_an_imported_title_is_unwrapped() {
    // The writer refuses a span nested in one of its own kind, so an import
    // that leaves one in the title cannot be written at all: the whole
    // document is lost, not the title.
    for (kind, html) in [
        ("toc", "<div class=\"toc\"><p class=\"admonition-title\">a <em>x <em>y</em></em> b</p><p>body</p></div>"),
        ("note", "<aside class=\"admonition note\"><p class=\"admonition-title\">a <em>x <em>y</em></em> b</p><p>body</p></aside>"),
    ] {
        let imported = carve::html_to_carve(html, &carve::HtmlImportOptions::default())
            .expect("the fragment imports");
        assert_eq!(
            imported.value,
            format!("::: {kind} \"a /x y/ b\"\nbody\n:::\n"),
            "{kind}: the nested emphasis was not unwrapped"
        );
    }
}

#[test]
fn the_prosemirror_bridge_carries_the_title() {
    // Under `title`, the name CarveKit's `carveDirective` declares (it reserves
    // the name from the authored run, so unlike an admonition's there is no
    // second `title` to confuse it with). Inventing a key would leave the way
    // back reading a name no other consumer writes.
    let doc = carve::parse("::: toc \"Contents\"\n:::\n");
    let pm = carve::to_prosemirror(&doc);
    assert!(
        pm.json.contains("\"title\":\"Contents\""),
        "the bridge dropped the title: {}",
        pm.json
    );
    let back = carve::from_prosemirror(&pm.json).expect("the payload decodes");
    assert_eq!(
        carve::render_carve(&back).expect("the way back writes"),
        "::: toc \"Contents\"\n\n:::\n"
    );
    // The control, and the pair that shows the assertion is about the title:
    // the admonition has always carried one.
    let note = carve::to_prosemirror(&carve::parse("::: note \"Contents\"\n:::\n"));
    assert!(note.json.contains("\"carveAdmonitionTitle\":\"Contents\""));
}
