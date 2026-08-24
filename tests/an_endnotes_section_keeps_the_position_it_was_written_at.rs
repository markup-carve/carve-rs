//! An endnotes section's POSITION is meaning, and an import keeps it
//! (markup-carve/carve#1627, `docs/html-import.md`; markup-carve/carve-rs#1313).
//!
//! The notes are consumed into footnote definitions and the renderer appends
//! the section it rebuilds at DOCUMENT END. That reproduces the input exactly
//! where the section was already last, and silently MOVES it where it was not:
//! the same characters in a different order, with nothing said.
//!
//! This is NOT `structure-unspellable` and there is nothing to report. Carve HAS
//! a spelling for the position - the `::: footnotes` placement directive - and
//! that is the whole argument: treating placement as a rendering artifact would
//! be defensible only if the language could not say otherwise, and it can.
//!
//! GETTING HERE NEEDED A SECOND CHANGE, because this engine derived no footnotes
//! at all under the `generic` adapter: the whole pass was gated to `word` and
//! `google-docs`. carve-js runs it under EVERY adapter and gates only the
//! ANCHOR-PAIR HEURISTIC on the word-processor adapters, so an anchor the
//! producer marked `role="doc-noteref"` binds everywhere. That is authored
//! DPUB-ARIA semantics rather than a guess - it is what carve-php's core policy
//! reads too - and it is what makes Pandoc 2.11+ output import its footnotes
//! without naming an adapter. This engine now matches. A role-less document
//! under `generic` imports exactly as it did before, which the last two tests
//! here pin from both sides.

const REFERENCE: &str =
    "<p>a<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n";
const SECTION: &str =
    "<section role=\"doc-endnotes\"><ol><li id=\"fn1\"><p>n</p></li></ol></section>\n";

fn imported(html: &str) -> carve::HtmlImportResult<String> {
    carve::html_to_carve(html, &carve::HtmlImportOptions::default()).expect("import")
}

fn codes(result: &carve::HtmlImportResult<String>) -> Vec<&str> {
    result
        .report
        .diagnostics
        .iter()
        .map(|d| d.code.as_str())
        .collect()
}

#[test]
fn a_section_that_is_not_last_keeps_its_position_as_a_placement_directive() {
    let result = imported(&format!("{REFERENCE}{SECTION}<p>after</p>"));
    assert_eq!(
        result.value,
        "a[^1]\n\n::: footnotes\n\n:::\n\nafter\n\n[^1]: n\n"
    );
    // Definitions are collected to document level whatever the source says,
    // which is why the definition is written last; the DIRECTIVE is what puts
    // the rendered section back where the HTML had it.
    assert!(codes(&result).is_empty(), "{:?}", codes(&result));
}

#[test]
fn the_written_source_renders_the_input_in_the_inputs_own_order() {
    // The point of the directive, stated as the round trip rather than as a
    // string: the notes render BEFORE the paragraph that followed them.
    let rendered = carve::to_html(&imported(&format!("{REFERENCE}{SECTION}<p>after</p>")).value);
    let notes_at = rendered.find("doc-endnotes").expect("a rebuilt section");
    let after_at = rendered.find("after").expect("the trailing paragraph");
    assert!(
        notes_at < after_at,
        "expected the notes before the trailing paragraph, got {rendered:?}"
    );
}

#[test]
fn a_section_that_is_last_gets_no_directive() {
    // Where the section IS last the definitions already render there, and
    // adding the directive would put a construct in the source the input did
    // not distinguish. Every document that was already right stays
    // byte-identical.
    let result = imported(&format!("{REFERENCE}{SECTION}"));
    assert_eq!(result.value, "a[^1]\n\n[^1]: n\n");
    assert!(codes(&result).is_empty(), "{:?}", codes(&result));
}

#[test]
fn last_is_asked_of_the_document_and_not_of_the_immediate_siblings() {
    // A section last in a `<div>` that is ITSELF followed by a paragraph is
    // still not last in the document, so the check runs OUTWARD through the
    // ancestors. Reading only the immediate siblings would call this one last.
    //
    // THE WRAPPER HAS TO SURVIVE THE PRUNE for this to ask the question. An
    // EMPTY `<div>` is pruned along with the section, so the slot the marker
    // takes is the div's own slot in the body, where `<p>after</p>` is an
    // immediate sibling and the climb never runs - a wrapper holding nothing
    // else pins nothing here, which is what a first draft of this test did.
    // The leading paragraph is what keeps the div, and therefore what makes
    // the ancestor step load-bearing.
    let result = imported(&format!(
        "{REFERENCE}<div><p>x</p>{SECTION}</div><p>after</p>"
    ));
    assert_eq!(
        result.value,
        "a[^1]\n\nx\n\n::: footnotes\n\n:::\n\nafter\n\n[^1]: n\n"
    );
}

#[test]
fn a_wrapper_the_prune_empties_puts_the_marker_in_the_wrappers_own_slot() {
    // The other half of the case above: with nothing else in the `<div>` the
    // prune walks through it, so the marker stands where the DIV stood. Same
    // written position, reached a different way, and worth pinning because the
    // two are easy to confuse for one another.
    let result = imported(&format!("{REFERENCE}<div>{SECTION}</div><p>after</p>"));
    assert_eq!(
        result.value,
        "a[^1]\n\n::: footnotes\n\n:::\n\nafter\n\n[^1]: n\n"
    );
}

#[test]
fn the_ast_exit_gets_the_placement_node_in_the_same_slot() {
    // PART 12: the two import exits agree. `html_to_carve` writes `:::
    // footnotes` where the section sat, so `html_to_ast` puts the node it is
    // written from in that same position rather than leaving the AST consumer
    // to rediscover it.
    let document = carve::html_to_ast(
        &format!("{REFERENCE}{SECTION}<p>after</p>"),
        &carve::HtmlImportOptions::default(),
    )
    .expect("import")
    .value;
    assert_eq!(document.children.len(), 3);
    let carve::BlockNode::Admonition(placement) = &document.children[1] else {
        panic!(
            "expected a placement admonition, got {:?}",
            document.children
        );
    };
    assert_eq!(placement.kind, "footnotes");
    assert!(placement.children.is_empty());
    assert!(document.footnote_defs.contains_key("1"));
}

#[test]
fn a_document_last_section_leaves_the_ast_without_a_placement_node() {
    let document = carve::html_to_ast(
        &format!("{REFERENCE}{SECTION}"),
        &carve::HtmlImportOptions::default(),
    )
    .expect("import")
    .value;
    assert!(
        !document
            .children
            .iter()
            .any(|child| matches!(child, carve::BlockNode::Admonition(_))),
        "{:?}",
        document.children
    );
}

/// A role-less mutual anchor pair: the shape only the heuristic binds.
const ROLELESS: &str = "<p>a<a id=\"fnref1\" href=\"#fn1\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\"><ol><li id=\"fn1\"><p>n</p><a href=\"#fnref1\">back</a></li></ol></section>\n<p>after</p>";

#[test]
fn a_role_less_pair_is_not_a_footnote_under_the_generic_adapter() {
    // The gate, from the side that must NOT move. `generic` takes arbitrary
    // HTML, where a mutually linked anchor pair is not proof of a footnote: an
    // unmarked anchor addressing a note is a LINK, and the document keeps the
    // author's shape.
    let written = imported(ROLELESS).value;
    assert!(
        !written.contains("[^1]"),
        "expected no derived footnote, got {written:?}"
    );
    assert!(
        !written.contains("::: footnotes"),
        "expected no placement directive, got {written:?}"
    );
}

#[test]
fn a_role_less_pair_is_still_a_footnote_under_a_word_processor_adapter() {
    // The other side of the same gate: naming an adapter is the declaration of
    // provenance that makes the pair heuristic safe, and this engine still
    // spends it there. Without this assertion the test above could be satisfied
    // by never deriving anything at all.
    let options = carve::HtmlImportOptions {
        adapter: carve::HtmlImportAdapter::Word,
        ..Default::default()
    };
    let written = carve::html_to_carve(ROLELESS, &options)
        .expect("import")
        .value;
    assert!(written.contains("a[^1]"), "{written:?}");
    assert!(written.contains("::: footnotes"), "{written:?}");
}
