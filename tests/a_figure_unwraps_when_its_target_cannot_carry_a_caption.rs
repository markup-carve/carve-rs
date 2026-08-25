//! In `safe` and `semantic`, a figure whose target cannot CARRY a caption line
//! unwraps and declares, instead of writing a line the target will absorb
//! (ruling markup-carve/carve-php#1731).
//!
//! THE ASSERTION THAT MATTERS IS ON THE RE-RENDER, not on the emitted Carve. A
//! test that pinned only the string would pass an implementation that still
//! writes `^ Cap` under prose, and that is exactly the defect: `^ Cap` is a
//! caption line only when the block above it can carry a caption, so a bare
//! paragraph reads it as more of the same paragraph and the caret survives as a
//! literal character. Before this rule the import wrote
//!
//! ```text
//! {#f .c}
//! x
//! ^ Cap
//! ```
//!
//! for `<figure id="f" class="c"><p>x</p><figcaption>Cap</figcaption></figure>`,
//! which renders `<p id="f" class="c">x ^ Cap</p>`: the figure gone, the caption
//! turned into prose, and a caret in the document nobody typed. A loss can be
//! declared and an ADDITION cannot, which is why this is a fix rather than a row.
//!
//! THE SET IS A PROPERTY, NOT A TAG LIST. `caption_line_binds` answers for every
//! mode; the modes differ only in what they do with a target outside it, and
//! `a_roundtrip_figure_rebuilds_only_where_a_carve_spelling_reproduces_it.rs`
//! pins the other side. So a paragraph and a `<div>` body take the same exit
//! without either being named, and a caption target added later inherits both
//! halves.

use carve::{html_to_carve, to_html, HtmlImportDiagnosticCode, HtmlImportMode, HtmlImportOptions};

const MODES: [HtmlImportMode; 2] = [HtmlImportMode::Safe, HtmlImportMode::Semantic];

const PARAGRAPH: &str =
    "<figure id=\"f\" class=\"c\"><p>x</p><figcaption>Cap</figcaption></figure>";
const DIV: &str = "<figure id=\"f\" class=\"c\"><div>x</div><figcaption>Cap</figcaption></figure>";
const MATH: &str = "<figure id=\"f\" class=\"c\"><p><span class=\"math display\">\\[x\\]</span></p><figcaption>Cap</figcaption></figure>";
const LIST: &str =
    "<figure id=\"f\" class=\"c\"><ul><li>x</li></ul><figcaption>Cap</figcaption></figure>";
const IMAGE: &str =
    "<figure id=\"f\" class=\"c\"><img src=\"a.png\" alt=\"A\"><figcaption>Cap</figcaption></figure>";
const QUOTE: &str = "<figure id=\"f\" class=\"c\"><blockquote><p>q</p></blockquote><figcaption>Cap</figcaption></figure>";
const CODE: &str =
    "<figure id=\"f\" class=\"c\"><pre><code>q</code></pre><figcaption>Cap</figcaption></figure>";

struct Imported {
    carve: String,
    codes: Vec<HtmlImportDiagnosticCode>,
    rendered: String,
}

fn import(html: &str, mode: HtmlImportMode) -> Imported {
    let options = HtmlImportOptions {
        mode,
        ..HtmlImportOptions::default()
    };
    let result = html_to_carve(html, &options).expect("imports");
    let rendered = to_html(&result.value);
    Imported {
        carve: result.value,
        codes: result
            .report
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect(),
        rendered,
    }
}

/// The reported shape, byte for byte against carve-php, which is the engine that
/// had it right: the body, a blank line, and the caption as its own paragraph,
/// with the element and each dropped attribute declared.
#[test]
fn a_paragraph_figure_unwraps_and_declares_what_it_cost() {
    let mut wrong = Vec::new();
    for mode in MODES {
        let imported = import(PARAGRAPH, mode);
        if imported.carve != "x\n\nCap\n" {
            wrong.push(format!("{mode:?}: wrote {:?}", imported.carve));
        }
        if imported.codes
            != vec![
                HtmlImportDiagnosticCode::ElementUnwrapped,
                HtmlImportDiagnosticCode::AttributeDropped,
                HtmlImportDiagnosticCode::AttributeDropped,
            ]
        {
            wrong.push(format!("{mode:?}: reported {:?}", imported.codes));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// THE CARET IS THE WHOLE POINT. Zero rows used to accompany a rendered
/// `<p id="f" class="c">x ^ Cap</p>`; nothing this importer writes may put a
/// character into the text that the author did not. A `<div>` body and a
/// display-math paragraph reach the same exit through different tags, and the
/// list is the control: it already unwrapped, and the engine used to warn
/// loudest on the harmless case and say nothing at all on the one that
/// corrupted the text.
#[test]
fn no_caret_reaches_the_rendered_text() {
    let mut wrong = Vec::new();
    for (name, html, expected) in [
        ("paragraph", PARAGRAPH, "x\n\nCap\n"),
        ("div body", DIV, "x\n\nCap\n"),
        ("display math", MATH, "$$`x`\n\nCap\n"),
        ("list", LIST, "- x\n\nCap\n"),
    ] {
        for mode in MODES {
            let imported = import(html, mode);
            if imported.carve != expected {
                wrong.push(format!("{name} in {mode:?}: wrote {:?}", imported.carve));
            }
            if imported.rendered.contains('^') {
                wrong.push(format!(
                    "{name} in {mode:?}: a caret reached the rendered text: {}",
                    imported.rendered
                ));
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// The targets that CAN carry a caption keep the rebuild, in these modes as in
/// `roundtrip`: the caption line re-parses to the figure it was written from, so
/// the element survives and there is nothing to declare.
#[test]
fn a_target_the_caption_line_binds_to_still_rebuilds() {
    let mut wrong = Vec::new();
    for (name, html) in [("image", IMAGE), ("quote", QUOTE), ("code block", CODE)] {
        for mode in MODES {
            let imported = import(html, mode);
            if !imported.codes.is_empty() {
                wrong.push(format!("{name} in {mode:?}: reported {:?}", imported.codes));
            }
            if !imported.rendered.contains("<figcaption>Cap</figcaption>") {
                wrong.push(format!(
                    "{name} in {mode:?}: did not read back as a figure: {}",
                    imported.rendered
                ));
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// THE ATTRIBUTES GO, DELIBERATELY. Landing the figure's `id` on the paragraph
/// would keep an anchor resolvable at one fewer declared loss, and it was
/// considered and rejected: the id identified a figure, and a bare paragraph
/// wearing it identifies something the author never marked.
#[test]
fn the_wrapper_attributes_are_dropped_rather_than_moved_onto_the_body() {
    for mode in MODES {
        let imported = import(PARAGRAPH, mode);
        assert!(
            !imported.carve.contains("{#f"),
            "{mode:?}: the id was written onto the body: {:?}",
            imported.carve
        );
        assert!(
            !imported.rendered.contains("id=\"f\""),
            "{mode:?}: the id reached the re-render: {}",
            imported.rendered
        );
    }
}

/// `roundtrip` is untouched by this ruling: it can keep the bytes, so it does
/// (markup-carve/carve#1704).
#[test]
fn roundtrip_still_preserves_the_whole_element() {
    let imported = import(PARAGRAPH, HtmlImportMode::Roundtrip);
    assert!(
        imported.carve.contains("```=html") && imported.carve.contains(PARAGRAPH),
        "{:?}",
        imported.carve
    );
    assert_eq!(imported.codes, vec![HtmlImportDiagnosticCode::RawPreserved]);
}
