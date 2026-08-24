//! `roundtrip` unwraps a generic sectioning wrapper instead of preserving it as
//! raw HTML (markup-carve/carve#1696).
//!
//! Nothing pinned this before, which is how an engine could raw-preserve EVERY
//! top-level heading in `roundtrip` with every gate green: this family renders
//! `# H` as `<section id="H"><h1>H</h1></section>`, so importing its own output
//! returned a document in which each heading was a ` ```=html ` block and none
//! of it was editable Carve any more.
//!
//! Both answers are fixed points - the raw block re-renders to the same bytes -
//! so a test that only asserted the round trip closes could not discriminate
//! between them. What discriminates is the SHAPE that comes back, which is what
//! every assertion here reads.

use carve::{html_to_carve, to_html, HtmlImportMode, HtmlImportOptions};

/// The seven the generic block arm reaches. `<div>` is not among them - it maps
/// to a Carve div - and `<figure>` is not either: it has its own per-target
/// roundtrip rule (markup-carve/carve#1704), and the control below holds it
/// there.
const SECTIONING: [&str; 7] = [
    "article", "aside", "footer", "header", "main", "nav", "section",
];

fn import(html: &str, mode: HtmlImportMode) -> String {
    let options = HtmlImportOptions {
        mode,
        ..HtmlImportOptions::default()
    };
    html_to_carve(html, &options).unwrap().value
}

#[test]
fn a_sectioning_wrapper_unwraps_in_every_mode() {
    // EVERY combination is EVALUATED, not just the first failing one. A loop
    // that asserts in place stops at its first panic, so the remaining tags and
    // modes never run at all - they did not pass, they were never measured, and
    // a fix covering one name would look like a fix covering seven.
    let mut wrong = Vec::new();
    for tag in SECTIONING {
        let html = format!("<{tag} id=\"x\"><h1>H</h1></{tag}>");
        for mode in [
            HtmlImportMode::Safe,
            HtmlImportMode::Semantic,
            HtmlImportMode::Roundtrip,
        ] {
            // `roundtrip` hands a `<section id>` back to the heading the
            // renderer hoisted it off (markup-carve/carve-rs#1380), so the
            // heading it comes back as carries the id. That is still the
            // heading Carve spells, which is what this test is about; the id
            // itself is ruled by `an_unwrapped_wrapper_hands_its_id_back_to_the_heading`.
            let hoisted = tag == "section" && mode == HtmlImportMode::Roundtrip;
            let want = if hoisted { "{#x}\n# H\n" } else { "# H\n" };
            let back = import(&html, mode);
            if back != want {
                wrong.push(format!("<{tag}> in {mode:?} -> {back:?}, want {want:?}"));
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "these must come back as the heading Carve spells, not as raw HTML:\n  {}",
        wrong.join("\n  ")
    );
}

#[test]
fn the_section_this_family_writes_for_a_heading_reads_back_as_that_heading() {
    // The exact shape the ticket is about, taken from the renderer rather than
    // written by hand: whatever wrapper `to_html` puts around a top-level
    // heading is the wrapper `roundtrip` has to see through.
    let html = to_html("# H\n");
    assert!(
        html.contains("<section"),
        "this fixture only means something while the renderer wraps a top-level \
         heading in a <section>; it wrote: {html}"
    );
    let back = import(&html, HtmlImportMode::Roundtrip);
    assert_eq!(back, "# H\n", "roundtrip did not recover the source");
    assert_eq!(
        to_html(&back),
        html,
        "the recovered source does not re-render to the HTML it came from"
    );
}

#[test]
fn a_wrapper_carve_cannot_spell_is_still_preserved_in_roundtrip() {
    // THE NEAR MISS, and the reason the fix is a list rather than "stop
    // raw-preserving block elements". `<figure>` reaches the same arm and must
    // keep reaching it for the targets no Carve spelling reproduces: a figure
    // around a PARAGRAPH writes a caption line that reads back as literal
    // prose, so unwrapping it trades a reported loss for a silent one
    // (carve#1286, narrowed to the target by markup-carve/carve#1704 - an
    // IMAGE figure does re-parse and rebuilds).
    let back = import(
        "<figure id=\"g\"><p>x</p><figcaption>Cap</figcaption></figure>",
        HtmlImportMode::Roundtrip,
    );
    assert!(
        back.contains("=html"),
        "<figure> must still be raw-preserved in roundtrip, got: {back:?}"
    );

    // And the arm is still live for an element that is not block at all, so a
    // fix that emptied it would fail here too.
    let inline = import("<marquee>x</marquee>", HtmlImportMode::Roundtrip);
    assert!(
        inline.contains("=html"),
        "an unsupported element must still be preserved in roundtrip, got: {inline:?}"
    );
}

#[test]
fn an_unwrapped_wrapper_still_reports_what_it_carried() {
    // Unwrapping is not a silent drop: what the `<section>` carried and has
    // nowhere to go is reported exactly as it is in the other two modes. A fix
    // that unwrapped quietly would turn a raw-preserve warning into a silent
    // loss, which is the opposite trade.
    //
    // The ID is no longer such a thing. `roundtrip` puts it back on the heading
    // the renderer hoisted it off (markup-carve/carve-rs#1380), so the fixture
    // asks about a CLASS, which has no slot to return to. The id case is the
    // control below.
    let options = HtmlImportOptions {
        mode: HtmlImportMode::Roundtrip,
        ..HtmlImportOptions::default()
    };
    let report = html_to_carve("<section class=\"kept\"><h1>H</h1></section>", &options).unwrap();
    assert_eq!(report.value, "# H\n");
    assert!(
        report
            .report
            .diagnostics
            .iter()
            .any(|d| d.message.contains("kept")),
        "the dropped class was not reported: {:?}",
        report.report.diagnostics
    );
}

/// The id the wrapper carries is the one thing the unwrap must NOT drop: with
/// `sections` on, the renderer hoisted the heading's id onto it, so unwrapping
/// and reporting it lost the author's `{#install}` from a mode whose job is
/// fidelity (markup-carve/carve-rs#1380).
#[test]
fn an_unwrapped_wrapper_hands_its_id_back_to_the_heading() {
    let options = HtmlImportOptions {
        mode: HtmlImportMode::Roundtrip,
        ..HtmlImportOptions::default()
    };
    let report = html_to_carve("<section id=\"install\"><h1>H</h1></section>", &options).unwrap();
    assert_eq!(report.value, "{#install}\n# H\n");
    assert!(
        !report
            .report
            .diagnostics
            .iter()
            .any(|d| d.message.contains("install")),
        "the id was restored but still reported dropped: {:?}",
        report.report.diagnostics
    );
}
