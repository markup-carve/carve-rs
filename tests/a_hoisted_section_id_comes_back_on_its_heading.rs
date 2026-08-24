//! `roundtrip` puts a `<section id>` back on the heading the renderer hoisted
//! it off, instead of unwrapping the wrapper and dropping it
//! (markup-carve/carve-rs#1380).
//!
//! carve-rs#1376 is right that a `<section>` is not a shape Carve cannot
//! express and that `roundtrip` should unwrap it rather than preserve it as
//! raw HTML. What that missed is WHAT the wrapper carries. With `sections` on,
//! `render_section` writes the heading's id on the wrapper and
//! `render_heading_without_section_id` leaves the heading without one - so the
//! single attribute on the wrapper is the single thing the import needs, and
//! unwrapping it dropped the author's id on the floor.
//!
//! Before carve-rs#1376 the same document round tripped, but only because the
//! whole wrapper was raw-preserved - the outcome carve#1696 rejected. Both
//! readings are wrong in different directions. The fix is to unwrap AND carry
//! the id back, which is the exact inverse of the hoist that put it there.
//!
//! The hard edge is telling the author's id from the renderer's. carve-rs#1355
//! reads two signals off a heading, its attribute POSITION and its VALUE, and
//! only one of them survives the hoist: the heading carries no id at all, so
//! slug equality is the whole test here. `{.k}` over `# H` renders
//! `<section id="H">` and must NOT come back as `{#H .k}`, which is a
//! different document - that is carve-rs#1355's ruling reaching through the
//! wrapper, and the control below is what holds it.

use carve::{html_to_carve, to_html, HtmlImportDiagnosticCode, HtmlImportMode, HtmlImportOptions};

struct Imported {
    carve: String,
    codes: Vec<HtmlImportDiagnosticCode>,
    messages: Vec<String>,
}

fn import(html: &str, mode: HtmlImportMode) -> Imported {
    let options = HtmlImportOptions {
        mode,
        ..HtmlImportOptions::default()
    };
    let result = html_to_carve(html, &options).expect("imports");
    Imported {
        carve: result.value,
        codes: result
            .report
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect(),
        messages: result
            .report
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.clone())
            .collect(),
    }
}

/// THE TICKET'S DOCUMENT. An authored id, hoisted onto the wrapper by the
/// render, has to come back on the heading - and the proof is the source, not
/// the bytes: both readings re-render to something, and only one of them is
/// the document that was written.
#[test]
fn an_authored_id_hoisted_onto_the_wrapper_comes_back_on_the_heading() {
    let source = "{#install .featured}\n## Setup\n";
    let html = to_html(source);
    assert!(
        html.contains("<section id=\"install\">") && !html.contains("<h2 id="),
        "this fixture only means something while the renderer hoists the id: {html}"
    );
    let imported = import(&html, HtmlImportMode::Roundtrip);
    assert_eq!(imported.carve, source, "the authored id did not come back");
    assert_eq!(
        to_html(&imported.carve),
        html,
        "the recovered source does not re-render to the HTML it came from"
    );
    // AND IT IS NOT REPORTED AS DROPPED, because it was not dropped. A row
    // saying otherwise would send a reader looking for a loss the output does
    // not have.
    assert!(
        !imported
            .codes
            .contains(&HtmlImportDiagnosticCode::AttributeDropped),
        "the id was restored but still reported dropped: {:?}",
        imported.messages
    );
}

/// CONTROL, and the reason the fix is not "keep every wrapper id".
/// carve-rs#1355 rules that a GENERATED heading id comes back generated, and
/// the hoist does not exempt it: `{.k}` over `# H` renders `<section id="H">`,
/// and writing that id back would spell an authored slot the source never had.
#[test]
fn a_derived_id_on_the_wrapper_is_still_left_to_the_renderer() {
    for source in ["{.k}\n# H\n", "# H\n"] {
        let html = to_html(source);
        let imported = import(&html, HtmlImportMode::Roundtrip);
        assert_eq!(
            imported.carve, source,
            "a derived section id was written back as an authored one"
        );
        assert_eq!(to_html(&imported.carve), html);
        // Dropping a value the renderer derives again is the no-op
        // `drop_derived` documents, so it is silent on purpose.
        assert!(
            !imported
                .codes
                .contains(&HtmlImportDiagnosticCode::AttributeDropped),
            "a derived id reported as a loss: {:?}",
            imported.messages
        );
    }
}

/// NESTING. Every level hoists, so every level has to come back - a fix that
/// only looked at the outermost wrapper would lose the inner ids.
#[test]
fn every_nested_wrapper_returns_its_own_id() {
    let source = "{#top}\n# A\n\n{#sub}\n## B\n";
    let html = to_html(source);
    let imported = import(&html, HtmlImportMode::Roundtrip);
    assert_eq!(imported.carve, source);
    assert_eq!(to_html(&imported.carve), html);
}

/// THE ID ONLY. `<section id>` is the whole of what the renderer writes on a
/// wrapper, so anything else on one is somebody's own markup: moving a class
/// onto the heading would render an attribute the input never had, on an
/// element that never had it. It keeps the `attribute-dropped` row it has
/// always had.
#[test]
fn only_the_id_moves_and_the_rest_is_still_reported() {
    let imported = import(
        "<section id=\"install\" class=\"c\"><h1>H</h1></section>",
        HtmlImportMode::Roundtrip,
    );
    assert_eq!(imported.carve, "{#install}\n# H\n");
    assert!(
        imported
            .messages
            .iter()
            .any(|m| m.contains("class=\"c\"") && m.contains("Dropped")),
        "the wrapper's class went missing without a word: {:?}",
        imported.messages
    );
    assert!(
        !imported
            .messages
            .iter()
            .any(|m| m.contains("id=\"install\"")),
        "the id was restored but still reported dropped: {:?}",
        imported.messages
    );
}

/// TWO IDS ARE TWO FACTS. A heading carrying its own id was never hoisted off,
/// so the wrapper's id is a second, different name - overwriting one with the
/// other would lose it, and it stays reported instead.
#[test]
fn a_heading_that_already_has_an_id_keeps_it() {
    let imported = import(
        "<section id=\"a\"><h1 id=\"b\">H</h1></section>",
        HtmlImportMode::Roundtrip,
    );
    assert_eq!(imported.carve, "{#b}\n# H\n");
    assert!(
        imported.messages.iter().any(|m| m.contains("id=\"a\"")),
        "the wrapper's id vanished without a word: {:?}",
        imported.messages
    );
}

/// A WRAPPER THAT IS NOT A HOIST. The id moves only off `<section>`, and only
/// onto a heading that is the wrapper's FIRST block - `render_section` writes
/// nothing else, so anything else is arbitrary markup whose id names a region
/// rather than a heading.
#[test]
fn nothing_moves_off_a_wrapper_the_renderer_never_writes() {
    let mut wrong = Vec::new();
    for html in [
        "<article id=\"a\"><h1>H</h1></article>",
        "<nav id=\"a\"><h1>H</h1></nav>",
        "<section id=\"a\"><p>x</p><h1>H</h1></section>",
    ] {
        let imported = import(html, HtmlImportMode::Roundtrip);
        if imported.carve.contains("{#a}") {
            wrong.push(format!("{html}\n  moved the id: {:?}", imported.carve));
        }
        if !imported.messages.iter().any(|m| m.contains("id=\"a\"")) {
            wrong.push(format!(
                "{html}\n  dropped it silently: {:?}",
                imported.messages
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "only render_section's own hoist is reversible:\n{}",
        wrong.join("\n")
    );
}

/// CONTROL: the other modes are untouched. `roundtrip` alone may read a
/// `<section id>` as a hoist, because its input is Carve-produced HTML by
/// definition; in arbitrary HTML the id names the region and the import has no
/// standing to move it onto a heading.
#[test]
fn the_other_modes_still_report_the_wrappers_id_as_dropped() {
    let mut wrong = Vec::new();
    for mode in [HtmlImportMode::Safe, HtmlImportMode::Semantic] {
        let imported = import("<section id=\"install\"><h1>H</h1></section>", mode);
        if imported.carve != "# H\n" {
            wrong.push(format!("{mode:?}: wrote {:?}", imported.carve));
        }
        if !imported.messages.iter().any(|m| m.contains("install")) {
            wrong.push(format!("{mode:?}: reported {:?}", imported.messages));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
