//! A container `[label]` publishes its INLINE RUN, not the characters the author
//! typed (`markup-carve/carve#2572`, corpus category 529).
//!
//! `CARVE-P9-041` used to ENUMERATE the hosts that have an inline run and named a
//! div label among them; `carve#2604` replaced the enumeration with a definition,
//! under which a container, link, span or fence label is a delimited region parsed
//! as `inline_content` in its own right.
//!
//! The old position was not tenable on its own terms either: every engine already
//! cuts a trailing `%%` comment inside a label, and that treatment exists only
//! because the label is a run. Accepting it for a comment marker and rejecting it
//! for emphasis is one host with two answers.
//!
//! The label STAYS A STRING in the AST, the way the oracle's own layout tree holds
//! it, so the scan happens where it is rendered and the interchange shape does not
//! move. `where_the_run_stops` pins what that costs.

use carve::{
    html_to_carve, parse, to_ansi, to_carve, to_html, to_markdown, to_plain_text, BlockNode,
    HtmlImportMode, HtmlImportOptions,
};

fn label_of(html: &str) -> String {
    let at = html
        .find("<p class=\"div-label\">")
        .unwrap_or_else(|| panic!("no div label in {html}"));
    let rest = &html[at + "<p class=\"div-label\">".len()..];
    rest[..rest.find("</p>").expect("unclosed div label")].to_string()
}

/// Corpus 529 and 529-2 through 529-7: the rows the release gate failed.
#[test]
fn the_corpus_rows_publish_their_run() {
    for (source, published) in [
        (":::[/i/]\nbody\n:::\n", "<em>i</em>"),
        (":::[*b*]\nbody\n:::\n", "<strong>b</strong>"),
        (":::[`x`]\nbody\n:::\n", "<code>x</code>"),
        (":::[plain]\nbody\n:::\n", "plain"),
        ("::: note [/i/]\nbody\n:::\n", "<em>i</em>"),
        ("::: note [`x`]\nbody\n:::\n", "<code>x</code>"),
        ("::: note [plain]\nbody\n:::\n", "plain"),
    ] {
        assert_eq!(label_of(&to_html(source)), published, "source: {source:?}");
    }
}

/// EVERY WRITER OF THE CAPTION FLOOR, not only the core one. Four publishers write
/// their own `<p class="div-label">` and each was escaping the label, so the floor
/// had one answer and they had another.
#[test]
fn every_publisher_of_the_floor_publishes_the_run() {
    for source in [
        ":::[/i/]\nbody\n:::\n",
        "::: note [/i/]\nbody\n:::\n",
        "::: spoiler \"T\" [/i/]\nbody\n:::\n",
        "::: list-table \"T\" [/i/]\n- - x\n  - y\n:::\n",
        "::: footnotes [/i/]\n:::\n\nx[^1]\n\n[^1]: n\n",
    ] {
        assert_eq!(
            label_of(&to_html(source)),
            "<em>i</em>",
            "source: {source:?}"
        );
    }
}

/// THE LABEL SLOT CLOSES WHERE THE READER'S OWN BRACKET SCAN CLOSES IT.
///
/// `label` takes a BALANCED run (`markup-carve/carve#2576`). The colon opener and
/// the fence info line are the two spellings of that one production, and both read
/// it up to the first `]` - so a `]` inside a verbatim span, an escaped one and a
/// nested bracket each cut the label short or turned the line into prose. Corpus
/// 529-8 is the fence row.
#[test]
fn the_label_slot_reads_a_balanced_run() {
    for (source, label) in [
        (":::[a `]` b]\nbody\n:::\n", "a <code>]</code> b"),
        ("::: note [a `]` b]\nbody\n:::\n", "a <code>]</code> b"),
        (":::[a \\] b]\nbody\n:::\n", "a ] b"),
        (":::[see [x](y)]\nbody\n:::\n", "see <a href=\"y\">x</a>"),
        (
            ":::[a [b]{.c} d]\nbody\n:::\n",
            "a <span class=\"c\">b</span> d",
        ),
    ] {
        assert_eq!(label_of(&to_html(source)), label, "source: {source:?}");
    }
    // The fence label does not surface as a caption, so this one is read off the
    // node: corpus 529-8 lost the language token with it.
    let doc = parse("``` js [a `]` b]\nc\n```\n");
    let Some(BlockNode::CodeBlock(code)) = doc.children.first() else {
        panic!("the fence did not open");
    };
    assert_eq!(code.lang.as_deref(), Some("js"));
    assert_eq!(code.label.as_deref(), Some("a `]` b"));
}

/// AN UNCLOSED VERBATIM RUN SWALLOWS THE CLOSER, so the line is prose - which is
/// what a link's text does with the same bytes, and the reason the slot is asked of
/// the reader rather than of a regex.
#[test]
fn an_unclosed_run_leaves_the_line_as_prose() {
    for source in [
        ":::[a ` b]\nbody\n:::\n",
        "::: note [a ` b]\nbody\n:::\n",
        "``` js [a ` b]\nc\n```\n",
    ] {
        assert!(
            !to_html(source).contains("div-label"),
            "source: {source:?} opened a labelled container: {}",
            to_html(source)
        );
    }
    // And an unbalanced one still leaves prose: the greedy read reaches the last
    // `]` and the trailing text is not empty.
    assert!(!to_html(":::[a] b]\nbody\n:::\n").contains("div-label"));
}

/// WHERE THE RUN STOPS, stated rather than discovered.
///
/// The label is a string in the AST, so a construct that needs a DOCUMENT PASS
/// over the tree cannot resolve in it: reference resolution, footnote numbering and
/// the abbreviation table all visit inline nodes, and there are none to visit. Each
/// publishes the text the author wrote, which is what every engine does with an
/// unresolved one anyway. carve-js stops in the same place.
#[test]
fn where_the_run_stops() {
    assert_eq!(
        label_of(&to_html(
            "[r]: https://e.test\n\n:::[a [r][] b]\nbody\n:::\n"
        )),
        "a [r][] b"
    );
    assert_eq!(
        label_of(&to_html(":::[a [^f] b]\nbody\n:::\n\n[^f]: note\n")),
        "a [^f] b"
    );
    assert_eq!(
        label_of(&to_html(
            "*[HTML]: HyperText\n\n:::[HTML here]\nbody\n:::\n"
        )),
        "HTML here"
    );
}

/// AND THE NON-HTML TARGETS STILL WRITE THE AUTHORED TEXT. They have no inline
/// renderer to hand a run to at this seam, so each writes the label as source -
/// which for the canonical writer is also the only thing that round trips.
#[test]
fn the_other_targets_write_the_label_as_authored() {
    let source = ":::[`x`]\nbody\n:::\n";
    assert_eq!(to_carve(source), "::: [`x`]\nbody\n:::\n");
    // Markdown writes the label as TEXT in a strong wrapper, so the backticks are
    // escaped rather than read as a span - the authored characters, not the run.
    assert!(
        to_markdown(source).starts_with("**\\`x\\`**"),
        "{}",
        to_markdown(source)
    );
    assert!(to_plain_text(source).starts_with("`x`"));
    assert!(to_ansi(source).contains("`x`"));
}

/// AN EXTENSION'S INLINE MATCHER IS NOT IN SCOPE FOR A LABEL. The run is scanned
/// at RENDER time from an AST field, with no parse of its own for a matcher to
/// belong to, and carve-js answers the same way by scanning a label through a
/// session that carries no matchers.
#[test]
fn an_extension_matcher_does_not_fire_in_a_label() {
    use carve::{render_html_with_options, Options, Wikilinks};

    let wikilinks = Wikilinks::new();
    let options = Options::new().with_extension(&wikilinks);
    let doc = carve::parse_with_options(":::[a [[Page]] b]\nbody [[Page]]\n:::\n", &options);
    let html = render_html_with_options(&doc, &options).expect("renders");

    // The same construct in the BODY does fire, which is what makes this a
    // measurement of the label rather than of the matcher.
    assert!(html.contains("class=\"wikilink\""), "{html}");
    assert_eq!(label_of(&html), "a [[Page]] b");
}

/// THE IMPORTER OWED THE OTHER HALF. A label the renderer degraded to a
/// `<p class="div-label">` is written back as the label's SOURCE, so a container
/// with a markup label survives the HTML round trip instead of coming back as a
/// `{.div-label}` paragraph - which is not a loss but an ADDITION, the document
/// saying something it never said.
#[test]
fn a_markup_label_survives_the_html_round_trip() {
    for source in [
        ":::[/i/]\nbody\n:::\n",
        ":::[*b*]\nbody\n:::\n",
        ":::[`x`]\nbody\n:::\n",
        "::: note [/i/]\nbody\n:::\n",
    ] {
        let options = HtmlImportOptions {
            mode: HtmlImportMode::Roundtrip,
            ..Default::default()
        };
        let html = to_html(source);
        // THE MARKUP HAS TO BE IN THE HTML for the return trip to be about
        // anything: an escaped label round trips as text either way, so the
        // detectors for the two halves are `the_corpus_rows_publish_their_run`
        // here and `a_markup_label_the_lift_accepts_keeps_the_fence` next door.
        assert!(
            html.contains("<em>") || html.contains("<strong>") || html.contains("<code>"),
            "source: {source:?} published no run: {html}"
        );
        let back = html_to_carve(&html, &options).expect("import").value;
        assert_eq!(to_html(&back), html, "source: {source:?}");
    }
}

/// AND NOTHING THE OPENER CANNOT SPELL RIDES BACK OUT. Each refusal leaves the
/// paragraph where it was, which is also what makes the three the near-miss
/// controls for the lift.
#[test]
fn a_label_the_opener_cannot_spell_is_not_lifted() {
    for html in [
        // A `]` closes the label, so the opener line would not read back.
        "<div><p class=\"div-label\">a]b</p><p>Body.</p></div>",
        // An empty `<code>` has no Carve source while its backtick run does not end.
        "<div><p class=\"div-label\">a <code></code> b</p><p>Body.</p></div>",
        // A link writes a `]` of its own.
        "<div><p class=\"div-label\">a <a href=\"u\">t</a> b</p><p>Body.</p></div>",
    ] {
        let options = HtmlImportOptions {
            mode: HtmlImportMode::Roundtrip,
            ..Default::default()
        };
        let back = html_to_carve(html, &options).expect("import").value;
        assert!(
            back.contains("{.div-label}"),
            "{html} lifted a label it cannot spell: {back}"
        );
    }
}

/// THE THROWAWAY WALK LEAVES NO TRACE. Writing the label's source walks the
/// paragraph's inline content to measure it; when the lift is then REFUSED, the
/// body walk reports the same loss, and a walk that did not rewind reported it
/// twice for one element.
#[test]
fn a_refused_lift_reports_its_loss_once() {
    let html =
        "<div><p class=\"div-label\">a <a href=\"u\">t</a> b <font>f</font></p><p>Body.</p></div>";
    let options = HtmlImportOptions {
        mode: HtmlImportMode::Roundtrip,
        ..Default::default()
    };
    let result = html_to_carve(html, &options).expect("import");
    let mut seen: Vec<String> = result
        .report
        .diagnostics
        .iter()
        .map(|d| format!("{:?}|{}", d.code, d.message))
        .collect();
    let before = seen.len();
    seen.sort();
    seen.dedup();
    assert_eq!(before, seen.len(), "a row was reported twice: {seen:?}");
}
