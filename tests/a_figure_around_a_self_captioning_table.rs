//! TWO CAPTIONS AND ONE SLOT (markup-carve/carve-rs#1402, ruling
//! markup-carve/carve-js#1488).
//!
//! A table brings its OWN caption slot, so a figure-wrapped table can arrive
//! carrying two captions - the table's `<caption>` and the figure's
//! `<figcaption>` - and Carve has one `^ ` line to spell them with. Writing
//! both was the INVENTION: the second line lands under a table that already
//! wrote one, so it is not a caption line at all and re-reads as a paragraph
//! holding a literal caret. The document came back with a `^` its author never
//! typed, in every mode, and the only row said the wrapper had no spelling.
//!
//! NEITHER CAPTION MAY BE THROWN AWAY EITHER. The figcaption is authored TEXT,
//! which is the one thing an import may not spend to reach a simpler shape. So
//! the two exits split the way markup-carve/carve#1704 already splits every
//! other figure: `roundtrip` PRESERVES the element, because no Carve spelling
//! reproduces it, and `safe` / `semantic` rebuild the table with its own
//! `<caption>` and write the figcaption as the PARAGRAPH after it. Both texts
//! survive either way; what the lossy modes spend is the caption ROLE, and one
//! row names it.
//!
//! THE TABLE IS THE ONLY TARGET WITH THIS COLLISION. A quote, a code block and
//! an image have no caption of their own, so the figure's line is uncontested
//! there - and those are pinned below as controls, because a fix that reached
//! them would be taking a caption slot nothing was competing for.
//!
//! ASSERTED ON THE RE-RENDER as well as on the written source: no caret may
//! reach the rendered text, and both caption strings have to still be in the
//! document.

use carve::{
    html_to_carve, to_html, HtmlImportDiagnosticCode, HtmlImportMode, HtmlImportOptions,
    HtmlImportSeverity,
};

const COLLIDING: &str = "<figure id=\"f\"><table><caption>TableCap</caption>\
                         <tr><td>a</td></tr></table><figcaption>FigCap</figcaption></figure>";

fn options(mode: HtmlImportMode) -> HtmlImportOptions {
    HtmlImportOptions {
        mode,
        ..Default::default()
    }
}

fn imported(html: &str, mode: HtmlImportMode) -> String {
    html_to_carve(html, &options(mode)).expect("import").value
}

fn rows(html: &str, mode: HtmlImportMode) -> Vec<(HtmlImportDiagnosticCode, String)> {
    html_to_carve(html, &options(mode))
        .expect("import")
        .report
        .diagnostics
        .iter()
        .map(|d| (d.code, d.path.clone().unwrap_or_default()))
        .collect()
}

/// THE CANARY. The cheapest assertion here that cannot hold unless the binary
/// linked the source shipped beside it, so a stale-artifact run fails here
/// first and names itself instead of looking like a behavior bug.
#[test]
fn the_detach_is_present_in_the_binary_under_test() {
    assert_eq!(
        imported(COLLIDING, HtmlImportMode::Semantic),
        "{#f}\n| a |\n^ TableCap\n\nFigCap\n",
        "a stale artifact: this binary's importer predates the double-caption detach"
    );
}

/// The ruled shape for the modes that cannot preserve.
#[test]
fn safe_and_semantic_detach_the_figcaption_into_a_paragraph() {
    for mode in [HtmlImportMode::Safe, HtmlImportMode::Semantic] {
        assert_eq!(
            imported(COLLIDING, mode),
            "{#f}\n| a |\n^ TableCap\n\nFigCap\n",
            "mode {mode:?}"
        );
    }
}

/// THE ADDITION IS GONE, which is what the ticket is about, and BOTH TEXTS ARE
/// STILL THERE, which is what stops the fix from being a drop.
#[test]
fn the_re_render_carries_both_captions_and_no_caret() {
    for mode in [HtmlImportMode::Safe, HtmlImportMode::Semantic] {
        let rendered = to_html(&imported(COLLIDING, mode));
        assert!(
            !rendered.contains('^'),
            "a caret reached the rendered document: {rendered}"
        );
        assert!(
            rendered.contains("<caption>TableCap</caption>"),
            "the table lost its own caption: {rendered}"
        );
        assert!(
            rendered.contains("<p>FigCap</p>"),
            "the figure's caption is not the paragraph after the table: {rendered}"
        );
        // AND THE FIGURE'S id RIDES ONTO THE TABLE, rather than going with the
        // wrapper: an anchor pointing at it still resolves.
        assert!(
            rendered.contains("<table id=\"f\">"),
            "the figure's id did not reach the table: {rendered}"
        );
        // DIRECTLY AFTER THE TABLE, which is what the row says.
        let table_end = rendered.find("</table>").expect("a table");
        let paragraph = rendered.find("<p>FigCap</p>").expect("the caption");
        assert!(paragraph > table_end, "the caption is not after the table");
    }
}

/// ONE ROW, and it names the `<figcaption>` rather than the wrapper - because
/// the reader has to be able to find the text, and the text is one block
/// further down as prose.
#[test]
fn one_row_names_the_figcaption_and_what_it_cost() {
    for mode in [HtmlImportMode::Safe, HtmlImportMode::Semantic] {
        assert_eq!(
            rows(COLLIDING, mode),
            vec![(
                HtmlImportDiagnosticCode::ElementUnwrapped,
                "/figure[1]/figcaption[2]".to_string()
            )],
            "mode {mode:?}"
        );
        let report = html_to_carve(COLLIDING, &options(mode))
            .expect("import")
            .report;
        let row = &report.diagnostics[0];
        assert_eq!(row.severity, HtmlImportSeverity::Warning);
        assert!(
            row.message.contains("Detached a <figcaption>")
                && row.message.contains("one caption slot"),
            "the row does not say what happened: {}",
            row.message
        );
    }
}

/// `roundtrip` PRESERVES, because no Carve spelling reproduces the element
/// (markup-carve/carve#1704). Both captions survive byte for byte.
#[test]
fn roundtrip_preserves_the_whole_figure() {
    let written = imported(COLLIDING, HtmlImportMode::Roundtrip);
    assert!(
        written.starts_with("```=html\n<figure id=\"f\">"),
        "not raw-preserved: {written}"
    );
    assert!(written.contains("<caption>TableCap</caption>"), "{written}");
    assert!(
        written.contains("<figcaption>FigCap</figcaption>"),
        "{written}"
    );
    assert_eq!(
        rows(COLLIDING, HtmlImportMode::Roundtrip)
            .into_iter()
            .map(|(code, _)| code)
            .collect::<Vec<_>>(),
        vec![HtmlImportDiagnosticCode::RawPreserved]
    );
}

/// BOTH CAPTIONS HAVE TO SPELL SOMETHING for there to be a collision at all,
/// and the test is on what the table WROTE rather than on what its `<caption>`
/// element holds - `<caption><span></span></caption>` is structurally non-empty
/// and writes no line, so the figure's caption takes the slot as it always did.
#[test]
fn an_empty_table_caption_leaves_the_slot_to_the_figure() {
    for html in [
        "<figure><table><caption></caption><tr><td>a</td></tr></table>\
         <figcaption>FigCap</figcaption></figure>",
        "<figure><table><caption> </caption><tr><td>a</td></tr></table>\
         <figcaption>FigCap</figcaption></figure>",
        "<figure><table><caption><span></span></caption><tr><td>a</td></tr></table>\
         <figcaption>FigCap</figcaption></figure>",
    ] {
        let written = imported(html, HtmlImportMode::Semantic);
        assert_eq!(written, "| a |\n^ FigCap\n", "{html}");
        // AND IT IS A CAPTION, not prose: the ordinary rebuild binds the line to
        // the table.
        assert!(
            to_html(&written).contains("<caption>FigCap</caption>"),
            "{html}"
        );
    }
}

/// AN EMPTY `<figcaption>` IS NOT A CAPTION TO DETACH, so the wrapper unwraps
/// and the table keeps its own.
#[test]
fn an_empty_figcaption_leaves_the_table_its_own_caption() {
    for html in [
        "<figure><table><caption>TableCap</caption><tr><td>a</td></tr></table>\
         <figcaption></figcaption></figure>",
        "<figure><table><caption>TableCap</caption><tr><td>a</td></tr></table>\
         <figcaption> </figcaption></figure>",
        "<figure><table><caption>TableCap</caption><tr><td>a</td></tr></table></figure>",
    ] {
        let written = imported(html, HtmlImportMode::Semantic);
        assert_eq!(written, "| a |\n^ TableCap\n", "{html}");
        assert!(!to_html(&written).contains('^'), "{html}");
    }
}

/// A TABLE WITH NO CAPTION OF ITS OWN is the ordinary rebuild and is untouched:
/// the figure's caption takes the one slot, and the wrapper's loss is the
/// `structure-unspellable` row it always had.
#[test]
fn a_table_with_no_caption_of_its_own_is_unchanged() {
    let html = "<figure><table><tr><td>a</td></tr></table><figcaption>FigCap</figcaption></figure>";
    assert_eq!(
        imported(html, HtmlImportMode::Semantic),
        "| a |\n^ FigCap\n"
    );
    assert_eq!(
        rows(html, HtmlImportMode::Semantic)
            .into_iter()
            .map(|(code, _)| code)
            .collect::<Vec<_>>(),
        vec![HtmlImportDiagnosticCode::StructureUnspellable]
    );
}

/// THE UNCONTESTED TARGETS. No other figure target has a caption of its own, so
/// none of them may lose its line to this change. Pinned rather than assumed:
/// the collision was swept for, not reasoned about.
#[test]
fn every_other_figure_target_keeps_its_caption_line() {
    for (html, written) in [
        (
            "<figure><blockquote><p>q</p></blockquote><figcaption>FigCap</figcaption></figure>",
            "> q\n^ FigCap\n",
        ),
        (
            "<figure><pre><code>x</code></pre><figcaption>FigCap</figcaption></figure>",
            "```\nx\n```\n^ FigCap\n",
        ),
        (
            "<figure><img src=\"a.png\" alt=\"A\"><figcaption>FigCap</figcaption></figure>",
            "![A](a.png)\n^ FigCap\n",
        ),
    ] {
        assert_eq!(imported(html, HtmlImportMode::Semantic), written, "{html}");
        assert_eq!(rows(html, HtmlImportMode::Semantic), Vec::new(), "{html}");
        // Each of these re-reads as the figure it was written from.
        assert!(
            to_html(written).contains("<figcaption>FigCap</figcaption>"),
            "{html}"
        );
    }
}

/// PROSE IS THE OTHER TARGET THE LINE DOES NOT BIND TO, and it took this exit
/// before this change did. It has to keep taking it - the detach must not have
/// captured the arm it shares.
#[test]
fn a_prose_target_still_detaches_as_it_did() {
    let html = "<figure><p>x</p><figcaption>FigCap</figcaption></figure>";
    let written = imported(html, HtmlImportMode::Semantic);
    assert_eq!(written, "x\n\nFigCap\n");
    assert!(!to_html(&written).contains('^'));
}

/// The figure and the table both setting a name is declared rather than
/// resolved in silence (markup-carve/carve#1721): the table's value wins the
/// merged line and one row says the figure's was displaced.
#[test]
fn a_name_both_sides_set_is_declared() {
    let html = "<figure id=\"f\"><table id=\"t\"><caption>TableCap</caption>\
                <tr><td>a</td></tr></table><figcaption>FigCap</figcaption></figure>";
    assert_eq!(
        imported(html, HtmlImportMode::Semantic),
        "{#t}\n| a |\n^ TableCap\n\nFigCap\n"
    );
    assert_eq!(
        rows(html, HtmlImportMode::Semantic)
            .into_iter()
            .map(|(code, _)| code)
            .collect::<Vec<_>>(),
        vec![
            HtmlImportDiagnosticCode::AttributeDropped,
            HtmlImportDiagnosticCode::ElementUnwrapped,
        ]
    );
    assert!(to_html(&imported(html, HtmlImportMode::Semantic)).contains("<table id=\"t\">"));
}
