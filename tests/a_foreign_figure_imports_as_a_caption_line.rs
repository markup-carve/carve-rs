//! The HTML importer keeps a foreign `<figure>` and a `<blockquote>`'s `cite`
//! (markup-carve/carve#1286, carve-rs#1027).
//!
//! Two independent halves of one importer, fixed together because both are the
//! same defect shape: an element or attribute the importer had no arm for, so
//! the generic path threw away something the target language can spell.
//!
//! FIGURE. Only this engine's OWN composite-figure classes routed to
//! `figure_panel`; any other `<figure>` fell through to the unsupported-element
//! unwrap, and the caption ran straight onto the content it captioned:
//!
//! ```text
//! <figure><img src="i.png" alt="a"><figcaption>cap</figcaption></figure>
//! ```
//!
//! imported as `![a](i.png)cap`, ONE paragraph, so re-parsing gave
//! `<p><img src="i.png" alt="a">cap</p>` - the figure gone rather than degraded.
//! The fix routes a foreign figure through the same `figure_panel` rebuild the
//! own-output panel already used, which carries the every-target mapping and the
//! multi-block fallback, so no second spelling of the mapping was added.
//!
//! CITE. A `<blockquote cite>` was dropped with an `attribute-dropped`
//! diagnostic. It is REPRESENTABLE, and MEASURED to round-trip losslessly in
//! this engine before it was kept: `{cite=u}` above `> q` renders
//! `<blockquote cite="u">` again. Keeping it costs nothing, and dropping it
//! costs the reader the provenance of the quote.
//!
//! MEASURED AGAINST carve-js, not assumed: its `htmlToCarve` returns
//! `![a](i.png)\n^ cap\n` for the figure, so carve-rs was the sole outlier
//! there. On the `cite` it currently agrees with the OLD carve-rs and drops the
//! attribute - carve-php keeps it - so this half is carve-rs moving to the ruled
//! answer ahead of carve-js rather than to the majority.
//!
//! ROUNDTRIP MODE IS EXCLUDED, and that exclusion is the interesting part. The
//! rebuild is only lossless for the targets the caption-line syntax re-parses.
//! Measured, one figure per target:
//!
//! - image, `<pre>` and `<blockquote>` come back as a `<figure>`.
//! - a `<table>` comes back as a `<table><caption>`, the Carve spelling for it.
//! - a bare PARAGRAPH would write `x` then `^ cap` and read back as
//!   `<p>x ^ cap</p>` - one paragraph of prose with the caption as literal text.
//! - a LIST detaches the caption into a paragraph of its own.
//!
//! Roundtrip mode promises the original bytes back for anything this engine
//! cannot guarantee, and says so with a `raw-preserved` warning. Taking the
//! rebuild there would have traded that documented warning for a silent
//! structural loss on the last two rows, so the arm is gated out of it and that
//! mode keeps exactly the behavior it had. carve-js converts in roundtrip mode
//! too and reports nothing, so this is carve-rs keeping a contract carve-js does
//! not make rather than a divergence introduced here.
//!
//! THE PARAGRAPH ROW LATER LEFT THE REBUILD ENTIRELY. Writing a caption line
//! prose absorbs is an ADDITION rather than a loss, and no mode is licensed to
//! make one, so ruling markup-carve/carve-php#1731 has the lossy modes unwrap
//! and declare where `roundtrip` preserves. The absorption measurement above is
//! the reason for both halves; only the alternatives differ by mode.

use carve::{html_to_ast, html_to_carve, HtmlImportError, HtmlImportMode, HtmlImportOptions};

fn import(html: &str) -> String {
    html_to_carve(html, &HtmlImportOptions::default())
        .expect("imports")
        .value
}

fn diagnostics(html: &str) -> Vec<String> {
    html_to_carve(html, &HtmlImportOptions::default())
        .expect("imports")
        .report
        .diagnostics
        .iter()
        .map(|d| d.message.clone())
        .collect()
}

fn ast_diagnostics(html: &str) -> Vec<String> {
    html_to_ast(html, &HtmlImportOptions::default())
        .expect("imports")
        .report
        .diagnostics
        .iter()
        .map(|d| d.message.clone())
        .collect()
}

/// The ruled shape: an image plus a caption line, not a concatenation.
#[test]
fn a_foreign_figure_imports_as_an_image_and_a_caption_line() {
    assert_eq!(
        import("<figure><img src=\"i.png\" alt=\"a\"><figcaption>cap</figcaption></figure>"),
        "![a](i.png)\n^ cap\n"
    );
}

/// The figure SURVIVES the round trip rather than degrading to a paragraph.
/// This is the assertion the old behavior failed on: it produced
/// `<p><img src="i.png" alt="a">cap</p>`.
#[test]
fn an_imported_figure_round_trips_to_a_figure() {
    let html = "<figure><img src=\"i.png\" alt=\"a\"><figcaption>cap</figcaption></figure>";
    let out = carve::to_html(&import(html));
    assert!(out.contains("<figure>"), "{out}");
    assert!(out.contains("<img src=\"i.png\" alt=\"a\">"), "{out}");
    assert!(out.contains("<figcaption>cap</figcaption>"), "{out}");
    assert!(
        !out.contains("<p>"),
        "the figure degraded back to a paragraph: {out}"
    );
}

/// Nothing is lost any more, so the importer stops reporting a loss. The two
/// `element-unwrapped` diagnostics were the honest signal of the old gap.
#[test]
fn the_figure_no_longer_reports_an_unwrapped_element() {
    let d =
        diagnostics("<figure><img src=\"i.png\" alt=\"a\"><figcaption>cap</figcaption></figure>");
    assert!(d.is_empty(), "{d:?}");
}

/// A `<figcaption>` written BEFORE the content still captions it, because the
/// caption is collected by tag rather than by position. Matches carve-js.
#[test]
fn a_caption_before_the_content_still_captions_it() {
    assert_eq!(
        import("<figure><figcaption>cap</figcaption><img src=\"i.png\" alt=\"a\"></figure>"),
        "![a](i.png)\n^ cap\n"
    );
}

/// A figure with no `<figcaption>` has no caption to write, so it imports as its
/// content alone. Pinned so a later change cannot start emitting an empty
/// caption line, which re-parses as literal text rather than as a figure.
#[test]
fn a_figure_without_a_caption_imports_as_its_content() {
    assert_eq!(
        import("<figure><img src=\"i.png\" alt=\"a\"></figure>"),
        "![a](i.png)\n"
    );
}

/// A `<blockquote>`'s `cite` survives as the attribute Carve spells for it.
#[test]
fn a_blockquote_keeps_its_cite() {
    assert_eq!(
        import("<blockquote cite=\"u\"><p>q</p></blockquote>"),
        "{cite=u}\n> q\n"
    );
}

/// And it round-trips: the kept attribute renders back onto the tag, which is
/// what makes keeping it lossless rather than merely well-meant.
#[test]
fn the_kept_cite_round_trips_onto_the_tag() {
    let out = carve::to_html(&import("<blockquote cite=\"u\"><p>q</p></blockquote>"));
    assert!(out.contains("<blockquote cite=\"u\">"), "{out}");
}

/// The attribute is no longer reported as dropped, since it is not.
#[test]
fn the_cite_is_no_longer_diagnosed_as_dropped() {
    let d = diagnostics("<blockquote cite=\"u\"><p>q</p></blockquote>");
    assert!(d.is_empty(), "{d:?}");
}

/// CONTROL: a blockquote WITHOUT a cite is unchanged - no attribute block
/// appears, and the import is byte-identical to what it was before. This is the
/// control that stops the allowlist entry leaking an empty `{}` onto every
/// quote.
#[test]
fn control_a_blockquote_without_a_cite_is_unchanged() {
    assert_eq!(import("<blockquote><p>q</p></blockquote>"), "> q\n");
    assert!(
        diagnostics("<blockquote><p>q</p></blockquote>").is_empty(),
        "a plain quote gained a diagnostic"
    );
}

/// `cite` is kept on a `<blockquote>` AND everywhere else. The tag-keyed
/// allowlist entry became unnecessary when the policy turned into a refusal
/// list: an attribute Carve can hold is kept wherever the author wrote it, and
/// nothing is reported because nothing is lost (carve-rs#1060).
#[test]
fn cite_is_kept_on_any_element_not_only_a_blockquote() {
    assert_eq!(import("<p cite=\"u\">q</p>"), "{cite=u}\nq\n");
    assert!(
        diagnostics("<p cite=\"u\">q</p>").is_empty(),
        "a kept attribute must not also be reported lost"
    );
    // The one exception is the element whose own MARKER owns the key: a
    // `<cite>` becomes `{cite}`, so a `cite` attribute on it has no room.
    assert!(
        diagnostics("<p><cite cite=\"u\">q</cite></p>")
            .iter()
            .any(|m| m.contains("cite")),
        "the marker collision must be named, not silently overwritten"
    );
}

/// CONTROL: this engine's OWN composite-figure shapes still route to their own
/// handlers. The new foreign-figure arm sits AFTER both class checks, so a
/// group is not swallowed by it.
#[test]
fn control_the_own_output_figure_group_still_round_trips() {
    let source = "\
{#fig-x .columns-2}
::: figure
{#fig-x-a}
![one](a.png)
^ (a) One
:::
^ Figure #: Group caption
";
    let html = carve::to_html(source);
    assert_eq!(carve::to_html(&import(&html)), html);
}

/// ROUNDTRIP MODE keeps the original bytes for a foreign figure NO CARVE
/// SPELLING REPRODUCES, with the `raw-preserved` warning it has always carried
/// (markup-carve/carve#1704 narrowed this from every figure to those).
#[test]
fn roundtrip_mode_still_preserves_a_figure_no_spelling_reproduces() {
    let options = HtmlImportOptions {
        mode: HtmlImportMode::Roundtrip,
        ..HtmlImportOptions::default()
    };
    for html in [
        "<figure><p>x</p><figcaption>cap</figcaption></figure>",
        "<figure><ul><li>a</li></ul><figcaption>cap</figcaption></figure>",
    ] {
        let result = html_to_carve(html, &options).expect("imports");
        assert!(
            result.value.contains("```=html"),
            "{html}: {}",
            result.value
        );
        assert!(result.value.contains(html), "{html}: {}", result.value);
        assert!(
            result
                .report
                .diagnostics
                .iter()
                .any(|d| d.message.contains("Preserved unsupported <figure>")),
            "{html}: the raw-preserved warning went missing"
        );
    }
}

/// The shape that made the gate necessary: a figure around a bare PARAGRAPH is
/// NOT losslessly representable, so `roundtrip` preserves the element rather
/// than converting it.
///
/// THE DEFAULT MODE USED TO CONVERT IT ANYWAY, and that was the defect ruling
/// markup-carve/carve-php#1731 settled. `x` then `^ cap` re-read as ONE
/// paragraph holding a literal caret, so the document gained a character its
/// author never wrote - and a lossy mode is licensed to LOSE the figure, never
/// to add to the text. It unwraps and declares instead, which is the shape
/// carve-php has always written.
#[test]
fn a_paragraph_figure_unwraps_by_default_and_is_preserved_in_roundtrip() {
    let html = "<figure><p>x</p><figcaption>cap</figcaption></figure>";
    assert_eq!(import(html), "x\n\ncap\n");
    assert!(
        !carve::to_html(&import(html)).contains('^'),
        "a caret reached the rendered text: {}",
        carve::to_html(&import(html))
    );
    assert_eq!(
        carve::to_html(&import(html)),
        "<p>x</p>\n<p>cap</p>",
        "the association is lost and every byte the author wrote is still theirs"
    );

    let options = HtmlImportOptions {
        mode: HtmlImportMode::Roundtrip,
        ..HtmlImportOptions::default()
    };
    let preserved = html_to_carve(html, &options).expect("imports").value;
    assert!(preserved.contains(html), "{preserved}");
}

/// A caption line has NO attribute slot, so a foreign `<figcaption>`'s
/// attributes cannot come with it - and the importer has to SAY so. Reading the
/// caption's children directly, as the rebuild first did, silently discarded an
/// `onclick`: the one attribute whose loss a reader most needs told, and exactly
/// the honesty the ticket credited this engine for. Found by a review pass on
/// this branch, not by the suite, which is why it is pinned here.
#[test]
fn a_foreign_captions_attributes_are_reported_rather_than_discarded() {
    let html = "<figure><img src=\"i.png\" alt=\"a\">\
<figcaption id=\"c\" class=\"k\" style=\"color:red\" onclick=\"x()\">cap</figcaption></figure>";
    let d = diagnostics(html);
    assert!(
        d.iter().any(|m| m.contains("onclick")),
        "an event handler was dropped with no diagnostic: {d:?}"
    );
    assert!(d.iter().any(|m| m.contains("CSS declarations")), "{d:?}");
    assert!(d.iter().any(|m| m.contains("id=\"c\"")), "{d:?}");
    assert!(d.iter().any(|m| m.contains("class=\"k\"")), "{d:?}");
    // The caption TEXT still arrives; reporting the attributes does not cost it.
    assert_eq!(import(html), "![a](i.png)\n^ cap\n");
}

/// The `<figcaption>` ELEMENT is charged against the import limits like any
/// other element. The rebuild first read its children without entering it, so a
/// caption slipped past `max_nodes` and one level of `max_depth` - a documented
/// bound quietly not being enforced on the one path foreign HTML now takes.
///
/// An EMPTY caption is what pins it. A caption with text costs more than no
/// caption either way, because the text charges on its own, so a test built on
/// that shape passes with the charge REMOVED - measured, and it is why this one
/// compares the empty caption instead. With the charge an empty `<figcaption>`
/// costs exactly one node more than no figcaption at all; without it, nothing.
#[test]
fn a_foreign_caption_element_is_charged_against_the_node_limit() {
    let budget = |html: &str| {
        (1..60)
            .find(|n| {
                html_to_carve(
                    html,
                    &HtmlImportOptions {
                        max_nodes: *n,
                        ..HtmlImportOptions::default()
                    },
                )
                .is_ok()
            })
            .expect("some budget suffices")
    };
    let no_caption = budget("<figure><img src=\"i.png\" alt=\"a\"></figure>");
    let empty_caption =
        budget("<figure><img src=\"i.png\" alt=\"a\"><figcaption></figcaption></figure>");
    assert_eq!(
        empty_caption,
        no_caption + 1,
        "the caption element must cost exactly its own node"
    );
    assert!(
        matches!(
            html_to_carve(
                "<figure><img src=\"i.png\" alt=\"a\"><figcaption></figcaption></figure>",
                &HtmlImportOptions {
                    max_nodes: empty_caption - 1,
                    ..HtmlImportOptions::default()
                },
            ),
            Err(HtmlImportError::NodeLimit)
        ),
        "the budget below the requirement must be refused"
    );
}

/// A rebuild that SUCCEEDS carries the figure's own attributes onto the node,
/// so the wrapper's id survives as the attribute block Carve spells for it.
#[test]
fn a_rebuilt_figure_keeps_the_wrappers_attributes() {
    let html =
        "<figure id=\"x\"><img src=\"i.png\" alt=\"a\"><figcaption>cap</figcaption></figure>";
    assert_eq!(import(html), "{#x}\n![a](i.png)\n^ cap\n");
    assert!(diagnostics(html).is_empty(), "nothing was lost");
    assert!(carve::to_html(&import(html)).contains("<figure id=\"x\">"));
}

/// A rebuild that FALLS BACK still announces the loss. `figure_panel` returns
/// bare blocks when there is no caption to bind, when the target is one the
/// caption line does not attach to, or when the body is several blocks - and in
/// each case the wrapper and its attributes are gone. Routing to the rebuild
/// made those cases silent, which is worse than the unwrap it replaced: the old
/// generic path at least said `element-unwrapped`.
#[test]
fn a_figure_that_cannot_be_rebuilt_still_reports_the_loss() {
    for html in [
        // no caption to bind
        "<figure id=\"x\"><img src=\"i.png\" alt=\"a\"></figure>",
        // a target the caption line does not attach to
        "<figure id=\"x\"><ul><li>a</li></ul><figcaption>cap</figcaption></figure>",
        // several body blocks
        "<figure id=\"x\"><p>a</p><p>b</p><figcaption>cap</figcaption></figure>",
    ] {
        let d = diagnostics(html);
        assert!(
            d.iter()
                .any(|m| m.contains("Unwrapped unsupported <figure>")),
            "{html}: the loss went unannounced: {d:?}"
        );
        assert!(
            d.iter().any(|m| m.contains("id=\"x\"")),
            "{html}: the wrapper's id vanished silently: {d:?}"
        );
    }
}

/// A FOREIGN figure keeps every class it carried. Only a class in FIRST
/// position identifies this engine's own panel, so `class="custom
/// carve-figure-panel"` is somebody else's markup that happens to spell the
/// name - and stripping it out would silently edit an author's class list.
/// Reusing the own-output rebuild is what made that possible, since it takes
/// the structural class off unconditionally.
#[test]
fn a_foreign_figure_keeps_a_class_that_merely_spells_the_structural_name() {
    assert_eq!(
        import(
            "<figure class=\"custom carve-figure-panel\"><img src=\"i.png\" alt=\"a\">\
<figcaption>cap</figcaption></figure>"
        ),
        "{.custom .carve-figure-panel}\n![a](i.png)\n^ cap\n"
    );
}

/// CONTROL: the class in FIRST position IS this engine's own output, so it is
/// structural and comes back off - the round trip must not gain a class the
/// author never wrote.
#[test]
fn control_the_own_output_panel_class_is_still_stripped() {
    assert_eq!(
        import(
            "<figure class=\"carve-figure-panel\"><img src=\"i.png\" alt=\"a\">\
<figcaption>cap</figcaption></figure>"
        ),
        "![a](i.png)\n^ cap\n"
    );
}

/// A PARAGRAPH-target figure USED TO BE A `Figure` IN THE AST that the writer
/// could not spell, and the loss was declared on the writing exit alone: the
/// tree kept a proper figure with its attributes, so an `html_to_ast` caller was
/// told nothing. That split is what `unspellable` exists to draw, and it is
/// gone from this shape - not because the split moved, but because there is no
/// such figure any more (ruling markup-carve/carve-php#1731). No caption line
/// binds to prose, so no figure is built from that target in any mode.
///
/// BOTH EXITS NOW SAY THE SAME THING, which is the property worth pinning: the
/// element unwrapped and the attributes it carried had nowhere to go. carve-php
/// and carve-js report the same pair from the same input.
#[test]
fn a_paragraph_target_figure_reports_the_same_loss_from_both_exits() {
    let html = "<figure id=\"x\"><p>x</p><figcaption>cap</figcaption></figure>";
    let d = diagnostics(html);
    assert!(
        d.iter()
            .any(|m| m.contains("Unwrapped unsupported <figure>")),
        "the figure left without a word: {d:?}"
    );
    assert!(
        d.iter().any(|m| m.contains("id=\"x\"")),
        "the id had nowhere to go and nothing said so: {d:?}"
    );
    assert_eq!(
        ast_diagnostics(html),
        d,
        "the AST no longer keeps a figure here, so both exits owe the same rows"
    );
    // The text is all still there; only the structure is gone.
    assert_eq!(import(html), "x\n\ncap\n");
}

/// A second `<figcaption>` must not cost the first one's TEXT. HTML allows at
/// most one, so a second is malformed markup - but the rebuild overwrote the
/// caption with it, and `one` disappeared from the document entirely. That is
/// content loss rather than a structural downgrade, and it is the one thing an
/// importer may never do quietly. The first NON-BLANK one captions; the extra falls
/// through to the host, where the generic unwrap keeps its text and reports it.
#[test]
fn a_second_caption_does_not_cost_the_first_ones_text() {
    let html = "<figure><figcaption>one</figcaption><img src=\"i.png\" alt=\"a\">\
<figcaption>two</figcaption></figure>";
    let out = import(html);
    assert!(
        out.contains("one"),
        "the first caption's text vanished: {out}"
    );
    assert!(
        out.contains("two"),
        "the second caption's text vanished: {out}"
    );
    // Byte-identical to carve-js, measured: the FIRST one captions and the extra
    // stays as content. That content makes the body a PARAGRAPH rather than a
    // lone image, and a caption line binds to no paragraph, so the figure
    // unwraps and the caption is written as its own block
    // (ruling markup-carve/carve-php#1731). What this test is about is that
    // neither caption's text is lost, and neither is.
    assert_eq!(out, "![a](i.png)two\n\none\n");
    assert!(
        diagnostics(html)
            .iter()
            .any(|m| m.contains("Unwrapped unsupported <figcaption>")),
        "the extra caption was absorbed without a word"
    );
}

/// A whitespace run between INLINE siblings is a word boundary the reader can
/// see. The own-output rebuild skips whitespace-only nodes because this engine's
/// pretty-printer puts margins between the wrapper and its host, and keeping
/// those leads the rebuilt image line with a space. A foreign figure gets no
/// such licence: dropping the space turned `<span>a</span> <span>b</span>` into
/// `ab`, which is a word joined that the author separated.
///
/// The line drawn is POSITION, not spelling. Leading and trailing whitespace in
/// a container is insignificant, and that is all a margin ever is; whitespace
/// BETWEEN siblings is a word boundary. A newline is not the tell - HTML
/// collapses a newline and a space alike to one space - so both spellings of the
/// separator are pinned here.
#[test]
fn a_foreign_figure_keeps_the_space_between_inline_siblings() {
    for html in [
        "<figure><span>a</span> <span>b</span><figcaption>cap</figcaption></figure>",
        "<figure><span>a</span>\n<span>b</span><figcaption>cap</figcaption></figure>",
    ] {
        // The body is prose, so the figure unwraps and the caption follows as
        // its own paragraph (ruling markup-carve/carve-php#1731). The subject
        // here is the SPACE between `a` and `b`, which survives either way.
        assert_eq!(import(html), "a b\n\ncap\n", "{html:?}");
    }
}

/// CONTROL: a pretty-printed figure still imports without the margin leaking
/// into the content. Keeping it would lead the image line with a space, and an
/// indented image line re-parses as prose rather than as an image.
#[test]
fn control_a_pretty_printed_figure_drops_its_margins() {
    assert_eq!(
        import(
            "<figure>\n  <img src=\"i.png\" alt=\"a\">\n  <figcaption>cap</figcaption>\n</figure>"
        ),
        "![a](i.png)\n^ cap\n"
    );
}

/// Whether the rebuild is LOSSLESS is a question about the written form, not
/// about the node - and WHERE it is said matters as much. `figure_panel` hands
/// back a `Figure` for targets the writer cannot spell as one, so taking the
/// node at face value made those the only lossy shapes that said nothing, while
/// reporting them as an IMPORT loss would lie to every `html_to_ast` caller.
/// One row per target, measured.
#[test]
fn each_target_reports_exactly_what_it_loses_and_only_where() {
    // Image, code block and quote read back as a figure. Nothing lost anywhere.
    for html in [
        "<figure id=\"x\"><img src=\"i.png\" alt=\"a\"><figcaption>cap</figcaption></figure>",
        "<figure id=\"x\"><pre><code>y</code></pre><figcaption>cap</figcaption></figure>",
        "<figure id=\"x\"><blockquote><p>q</p></blockquote><figcaption>cap</figcaption></figure>",
    ] {
        assert!(
            diagnostics(html).is_empty(),
            "{html}: {:?}",
            diagnostics(html)
        );
        assert!(
            carve::to_html(&import(html)).contains("<figure id=\"x\">"),
            "{html}"
        );
    }

    // A TABLE writes its caption onto the table and reads back as a
    // `<table><caption>`: the caption and the attributes survive, on the TABLE.
    // A writer-stage loss, so the AST caller hears nothing.
    let table =
        "<figure id=\"x\"><table><tr><td>c</td></tr></table><figcaption>cap</figcaption></figure>";
    assert!(
        diagnostics(table)
            .iter()
            .any(|m| m.contains("no Carve spelling")),
        "the wrapper went without a word: {:?}",
        diagnostics(table)
    );
    assert!(
        ast_diagnostics(table).is_empty(),
        "{:?}",
        ast_diagnostics(table)
    );
    assert!(
        carve::to_html(&import(table)).contains("id=\"x\""),
        "the id survived"
    );

    // A figure with NO caption is a real IMPORT loss - the AST has no figure
    // either - so it is reported on BOTH paths, and as a drop rather than as an
    // unspellable structure.
    let bare = "<figure id=\"x\"><img src=\"i.png\" alt=\"a\"></figure>";
    for d in [diagnostics(bare), ast_diagnostics(bare)] {
        assert!(
            d.iter()
                .any(|m| m.contains("Unwrapped unsupported <figure>")),
            "{d:?}"
        );
        assert!(d.iter().any(|m| m.contains("id=\"x\"")), "{d:?}");
    }
}

/// A COMMENT is invisible, so a margin does not stop being a margin for sitting
/// on the far side of one. Trimming only text left `<figure><!--x--> <img>`
/// holding a stray space beside the image, which made the host a paragraph
/// rather than an image: the OUTPUT was unchanged, but the figure was then
/// reported as unwrapped when it had not been. A false diagnostic costs the
/// reader as much as a missing one.
#[test]
fn a_boundary_comment_does_not_turn_a_margin_into_content() {
    for html in [
        "<figure><!--x-->\n<img src=\"i.png\" alt=\"a\"><figcaption>cap</figcaption></figure>",
        "<figure><img src=\"i.png\" alt=\"a\">\n<!--x--><figcaption>cap</figcaption></figure>",
    ] {
        assert_eq!(import(html), "![a](i.png)\n^ cap\n", "{html:?}");
        assert!(
            diagnostics(html).is_empty(),
            "{html:?}: nothing was lost, so nothing may be reported: {:?}",
            diagnostics(html)
        );
        assert!(
            carve::to_html(&import(html)).contains("<figure>"),
            "{html:?}"
        );
    }
}

/// A node dropped as a margin never reaches `blocks`, so nothing would charge
/// it - and a margin is the cheapest thing an author can repeat. Discarding a
/// node is not the same as never having walked it, so trimming spends the budget
/// too and `<figure>` gains no free ride the rest of the importer does not give.
#[test]
fn trimmed_figure_margins_are_charged_against_the_node_limit() {
    let budget = |html: &str| {
        (1..80)
            .find(|n| {
                html_to_carve(
                    html,
                    &HtmlImportOptions {
                        max_nodes: *n,
                        ..HtmlImportOptions::default()
                    },
                )
                .is_ok()
            })
            .expect("some budget suffices")
    };
    let plain = "<figure><img src=\"i.png\" alt=\"a\"><figcaption>cap</figcaption></figure>";
    let commented = "<figure><!--a--><!--b--><!--c--><img src=\"i.png\" alt=\"a\">\
<figcaption>cap</figcaption></figure>";
    assert_eq!(
        budget(commented),
        budget(plain) + 3,
        "three boundary comments must cost three nodes"
    );
    // And the output is unaffected by them: they are margins, not content.
    assert_eq!(import(commented), import(plain));
}

/// An EMPTY caption is not a caption. Kept as one it wrote a bare `^` line,
/// which re-parses as a literal caret inside a paragraph: the figure destroyed
/// AND a character in the output the author never typed. That is worse than any
/// other shape here - the rest lose structure, this one INVENTS content - and it
/// said nothing while doing it.
///
/// carve-js writes the bare `^` for this input, so carve-rs diverges from it
/// knowingly: matching it would mean reproducing the corruption.
#[test]
fn an_empty_caption_is_treated_as_absent_rather_than_written_bare() {
    for html in [
        "<figure><img src=\"i.png\" alt=\"a\"><figcaption></figcaption></figure>",
        "<figure><img src=\"i.png\" alt=\"a\"><figcaption>   </figcaption></figure>",
    ] {
        let out = import(html);
        assert_eq!(out, "![a](i.png)\n", "{html}");
        assert!(
            !carve::to_html(&out).contains('^'),
            "{html}: a caret the author never typed reached the output"
        );
        assert!(
            diagnostics(html)
                .iter()
                .any(|m| m.contains("Unwrapped unsupported <figure>")),
            "{html}: the wrapper went without a word: {:?}",
            diagnostics(html)
        );
    }
}

/// Emptiness is asked STRUCTURALLY, of the node list, and not by flattening the
/// caption to text. A flattener answers only for the node kinds it has an arm
/// for, and the renderer's has none for `Span`, so a caption reading
/// `<span class="label">Caption</span>` flattened to the empty string - and the
/// empty-caption rule above, meant to stop a bare caret, threw the caption's
/// TEXT away instead. The narrow fix for one corruption opened a wider one.
#[test]
fn a_caption_wrapped_in_a_span_is_not_mistaken_for_an_empty_one() {
    for (html, want) in [
        (
            "<figure><img src=\"i.png\" alt=\"a\">\
<figcaption><span class=\"label\">Caption</span></figcaption></figure>",
            "![a](i.png)\n^ [Caption]{.label}\n",
        ),
        (
            "<figure><img src=\"i.png\" alt=\"a\"><figcaption><em>Cap</em></figcaption></figure>",
            "![a](i.png)\n^ /Cap/\n",
        ),
    ] {
        assert_eq!(import(html), want, "{html}");
        assert!(
            diagnostics(html).is_empty(),
            "{html}: nothing was lost: {:?}",
            diagnostics(html)
        );
    }
}

/// A BLANK first caption is absent, so a later one is the only caption the
/// figure has and it takes the role - non-blank rather than merely first. The
/// alternative, demoting a visible string to content because an empty tag came
/// before it, loses nothing but reads the author's intent backwards.
#[test]
fn a_blank_first_caption_lets_a_later_one_caption() {
    assert_eq!(
        import(
            "<figure><figcaption></figcaption><img src=\"i.png\" alt=\"a\">\
<figcaption>two</figcaption></figure>"
        ),
        "![a](i.png)\n^ two\n"
    );
}
