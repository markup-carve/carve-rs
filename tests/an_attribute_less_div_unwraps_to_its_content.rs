//! An attribute-less `<div>` unwraps to its content; one attribute brings the
//! fence back.
//!
//! The HTML importer's `div` arm wrote a `:::` fence for every `<div>` it met,
//! attributes or not, so `<div>z</div>` came back as three lines where one says
//! the same thing. carve-js and carve-php both unwrap it, and the ruling on
//! markup-carve/carve#1578 sided with them: a bare `<div>` carries no meaning
//! of its own, so the fence buys the reader nothing and costs two lines of
//! markup nobody asked for. The element not surviving the round trip is the
//! honest outcome, because there is nothing in it to survive.
//!
//! The BOUNDARY is the whole rule, so both sides of it are pinned here rather
//! than left to fall out of the code. An attribute-less div unwraps; the moment
//! a div carries an attribute the language can hold, the fence comes back,
//! because then there IS something only the container can hold.
//!
//! Both halves matter and neither is enough alone. Pinning only the unwrap
//! would let the fence quietly stop being written for a div that needs it, and
//! pinning only the fence would let the unwrap regress to a fence around
//! nothing.
//!
//! The tests on `a_non_li_child_of_a_list_is_reported_and_kept` are deliberately
//! NOT reused: they use an id-bearing div precisely so they pin the list arm.
//! This shape is the div arm, and it was unpinned by any test or golden in this
//! engine before this file.

use carve::{
    html_to_carve, parse, render_html, HtmlImportDiagnosticCode, HtmlImportMode, HtmlImportOptions,
};

fn imported(html: &str) -> String {
    html_to_carve(html, &HtmlImportOptions::default())
        .unwrap()
        .value
}

fn diagnostics(html: &str) -> Vec<(String, String)> {
    html_to_carve(html, &HtmlImportOptions::default())
        .unwrap()
        .report
        .diagnostics
        .iter()
        .map(|d| {
            (
                d.code.as_str().to_string(),
                d.path.clone().unwrap_or_default(),
            )
        })
        .collect()
}

fn reparsed(src: &str) -> String {
    render_html(&parse(src)).unwrap()
}

// -- the attribute-less shape unwraps ---------------------------------------

/// The shape from the ticket, standalone and with no list anywhere near it.
/// The assertion goes through the parser as well as the emitted source: what
/// the unwrap promises is a document that still holds the content, and source
/// that did not read back as that document would satisfy a string assertion
/// while failing the promise.
#[test]
fn a_bare_div_unwraps_to_its_content() {
    assert_eq!(imported("<div>z</div>"), "z\n");
    assert_eq!(reparsed(&imported("<div>z</div>")), "<p>z</p>");
}

/// The measured case from the ticket, where the list arm hands the stray child
/// to the ordinary block walk. The content survives and so does the report that
/// the child left its place among the items - the unwrap changes what the div
/// is WRITTEN as, and must not swallow what the list arm has to say about it.
#[test]
fn a_bare_div_in_a_list_unwraps_and_is_still_reported() {
    assert_eq!(imported("<ul><div>z</div><li>a</li></ul>"), "z\n\n- a\n");
    assert_eq!(
        diagnostics("<ul><div>z</div><li>a</li></ul>"),
        vec![(
            HtmlImportDiagnosticCode::ElementUnwrapped
                .as_str()
                .to_string(),
            "/ul[1]/div[1]".to_string(),
        )],
    );
}

/// Several blocks keep their boundary. An unwrap that folded them into one
/// paragraph would keep the words and lose the document.
#[test]
fn a_bare_div_around_several_blocks_keeps_the_boundary() {
    assert_eq!(imported("<div><p>a</p><p>b</p></div>"), "a\n\nb\n");
    assert_eq!(reparsed("a\n\nb\n"), "<p>a</p>\n<p>b</p>");
}

/// Nesting unwraps all the way down rather than one level: the inner div is as
/// bare as the outer one, so neither is written. The old output widened the
/// fence to `::::` to hold a div that says nothing.
#[test]
fn nested_bare_divs_unwrap_all_the_way_down() {
    assert_eq!(imported("<div><div>z</div></div>"), "z\n");
}

/// An empty one writes no fence around no content.
#[test]
fn an_empty_bare_div_writes_nothing() {
    assert_eq!(imported("<div></div>"), "\n");
}

/// Not conditioned on the import mode. Roundtrip mode promises the original
/// bytes back for shapes this engine cannot spell, and an attribute-less div is
/// not one of those: nothing about it is unspellable, there is simply nothing
/// to spell.
#[test]
fn the_unwrap_does_not_depend_on_the_import_mode() {
    for mode in [
        HtmlImportMode::Safe,
        HtmlImportMode::Semantic,
        HtmlImportMode::Roundtrip,
    ] {
        let opts = HtmlImportOptions {
            mode,
            ..HtmlImportOptions::default()
        };
        assert_eq!(
            html_to_carve("<div>z</div>", &opts).unwrap().value,
            "z\n",
            "mode {mode:?} wrote a fence for an attribute-less div",
        );
    }
}

// -- one attribute brings the fence back ------------------------------------

/// The other side of the boundary, and the case both engines already agreed on
/// byte for byte before this fix. An id has nowhere else to live, so the
/// container is written to carry it.
#[test]
fn an_id_brings_the_fence_back() {
    assert_eq!(imported("<div id=\"x\">z</div>"), "{#x}\n:::\nz\n:::\n");
    assert_eq!(
        reparsed(&imported("<div id=\"x\">z</div>")),
        "<div id=\"x\">\n  <p>z</p>\n</div>"
    );
}

/// Any attribute, not only an id. A key-value pair the language can hold puts
/// the fence back for the same reason.
#[test]
fn a_key_value_attribute_brings_the_fence_back() {
    assert_eq!(
        imported("<div data-x=\"1\">z</div>"),
        "{data-x=1}\n:::\nz\n:::\n"
    );
}

/// The boundary inside a list, which is the shape the ticket was measured on.
/// This is the output that must not move: the fix changes the attribute-LESS
/// arm and leaves this one exactly where markup-carve/carve-rs#1266 put it.
#[test]
fn an_id_bearing_div_in_a_list_is_undisturbed() {
    assert_eq!(
        imported("<ul><div id=\"stray\">z</div><li>a</li></ul>"),
        "{#stray}\n:::\nz\n:::\n\n- a\n"
    );
}

/// The test is the ATTRIBUTE, not the markup. A `style` whose declarations the
/// style map refuses leaves the div carrying nothing, so it unwraps like any
/// other bare div - and the refusal is still reported, so the reader learns the
/// declaration went nowhere.
#[test]
fn an_attribute_the_language_cannot_hold_does_not_bring_the_fence_back() {
    assert_eq!(imported("<div style=\"color:red\">z</div>"), "z\n");
    assert_eq!(
        diagnostics("<div style=\"color:red\">z</div>"),
        vec![(
            HtmlImportDiagnosticCode::StyleUnmapped.as_str().to_string(),
            "/div[1]".to_string(),
        )],
    );
}

/// A class that names a container is a THIRD answer and is not touched by
/// either arm: it takes the admonition path above the unwrap, so it neither
/// unwraps nor writes a bare fence.
#[test]
fn a_container_class_still_takes_the_container_arm() {
    assert_eq!(
        imported("<div class=\"note\">z</div>"),
        "::: note\nz\n:::\n"
    );
}
