//! PART 11 §10f: a REFERENCED abbreviation definition splits by target.
//!
//! §10a rules the definition nothing references - every non-HTML target emits
//! it, because those targets do not get to drop content the author wrote. §10f
//! rules the one that IS referenced:
//!
//! - T1 MARKDOWN KEEPS THE LINE and the expansion beside it. `*[TERM]: expansion`
//!   is PHP Markdown Extra's own spelling, so there the line is CONTENT rather
//!   than leaked source, and keeping it is what lets the export round-trip.
//! - T2 PLAIN TEXT AND THE TERMINAL DROP THE LINE and emit only the expansion,
//!   in the `TERM (expansion)` shape, at every occurrence.
//!
//! The canonical writer keeps every line whatever became of it, because PART 11
//! §1's `parse(fmt(x)) == parse(x)` requires it - the opposite direction from
//! plain and the terminal, so it is asserted here rather than assumed.
//!
//! THE OPERATIVE TEST IS WHETHER THIS DEFINITION'S EXPANSION IS EMITTED, not
//! whether its term appears: the line goes because the content is emitted TWICE,
//! and it is emitted twice only where the expansion is emitted. The two shapes
//! that separate those readings - a later definition of the same term winning,
//! and an authored `abbr` outranking the definition - are pinned here and in
//! `authored_abbr_on_every_target.rs` respectively.
//!
//! Thirteen of the eighteen corpus cases carrying a definition have no non-HTML
//! sidecar, so `corpus_render_fixtures.rs` cannot see them at all. The shapes
//! among them that this rule turns on are pinned here instead.
//!
//! Every expectation is byte-exact. A `contains` assertion cannot fail on this
//! change in the direction that matters: before it, the definition line AND the
//! expansion were both present in the plain and terminal output, just in the
//! wrong place, so "output contains the expansion" passed while reverted.
//!
//! The terminal's dim styling is written as `\u{1b}` escapes. Every expectation
//! below was read back from a release render and its bytes checked with a hex
//! dump - `1b 5b 32 6d` for SGR 2 and `1b 5b 30 6d` for SGR 0 - rather than
//! pasted from a terminal, where an escape is invisible.

/// SGR 2, the terminal target's dim opener.
const DIM: &str = "\u{1b}[2m";
/// SGR 0, its reset.
const OFF: &str = "\u{1b}[0m";

fn dim(text: &str) -> String {
    format!("{DIM}{text}{OFF}")
}

// --- the basic shape (corpus `43-abbreviations`) ---------------------------

const BASIC: &str = "*[HTML]: HyperText Markup Language\n\nThe HTML spec is essential reading.\n";

#[test]
fn plain_drops_the_line_and_prints_the_expansion() {
    assert_eq!(
        carve::to_plain_text(BASIC),
        "The HTML (HyperText Markup Language) spec is essential reading.\n"
    );
}

#[test]
fn the_terminal_drops_the_line_and_keeps_the_expansion_dim() {
    assert_eq!(
        carve::to_ansi(BASIC),
        format!(
            "The HTML{} spec is essential reading.\n",
            dim(" (HyperText Markup Language)")
        )
    );
}

/// T1, and the reason the duplication is paid on this target and no other.
#[test]
fn markdown_keeps_both_the_line_and_the_expansion() {
    assert_eq!(
        carve::to_markdown(BASIC),
        "*[HTML]: HyperText Markup Language\n\n\
         The <abbr title=\"HyperText Markup Language\">HTML</abbr> spec is essential reading.\n"
    );
}

/// The canonical writer is not a presentation target: PART 11 §1's round trip
/// needs the line back.
#[test]
fn the_canonical_writer_keeps_the_line() {
    assert_eq!(
        carve::to_carve(BASIC),
        "*[HTML]: HyperText Markup Language\n\nThe HTML spec is essential reading.\n"
    );
}

// --- §10a, the control that must NOT move ----------------------------------

/// Corpus `70-blocks-that-render-to-nothing-3`. Nothing references the term, so
/// the definition's own line is the only thing carrying its text and every
/// non-HTML target still emits it.
#[test]
fn an_unreferenced_definition_still_survives_every_non_html_target() {
    let source = "*[HTML]: HyperText Markup Language\n\n:::\nbody\n:::\n";
    assert_eq!(
        carve::to_plain_text(source),
        "*[HTML]: HyperText Markup Language\n\nbody\n"
    );
    assert_eq!(
        carve::to_ansi(source),
        format!("{}\n\nbody\n", dim("*[HTML]: HyperText Markup Language"))
    );
    assert_eq!(
        carve::to_markdown(source),
        "*[HTML]: HyperText Markup Language\n\nbody\n"
    );
}

/// The same term defined twice, only one of them referenced. The decision is per
/// DEFINITION, so one line goes and the other stays in the same document.
#[test]
fn only_the_referenced_definition_loses_its_line() {
    let source =
        "*[HTML]: HyperText Markup Language\n*[CSS]: Cascading Style Sheets\n\nOnly HTML appears.\n";
    assert_eq!(
        carve::to_plain_text(source),
        "*[CSS]: Cascading Style Sheets\n\nOnly HTML (HyperText Markup Language) appears.\n"
    );
    assert_eq!(
        carve::to_ansi(source),
        format!(
            "{}\n\nOnly HTML{} appears.\n",
            dim("*[CSS]: Cascading Style Sheets"),
            dim(" (HyperText Markup Language)")
        )
    );
}

// --- PART 9R R3, last wins (corpus `175-a-repeated-definition-which-one-wins-2`)

const REPEATED: &str = "*[A]: a\n*[A]: b\n\nA here.\n";

/// Only `b` is ever emitted, so `*[A]: b` goes and `*[A]: a` stays.
///
/// This is why the decision matches on the `(term, expansion)` PAIR rather than
/// the term: keying on the term alone would drop both lines and delete the
/// string `a` from the document outright, which is the content loss §10a exists
/// to prevent and which §10f explicitly considered and rejected.
#[test]
fn the_losing_definition_keeps_its_line_on_plain() {
    assert_eq!(carve::to_plain_text(REPEATED), "*[A]: a\n\nA (b) here.\n");
}

#[test]
fn the_losing_definition_keeps_its_line_on_the_terminal() {
    assert_eq!(
        carve::to_ansi(REPEATED),
        format!("{}\n\nA{} here.\n", dim("*[A]: a"), dim(" (b)"))
    );
}

#[test]
fn markdown_and_the_canonical_writer_keep_both_repeated_lines() {
    assert_eq!(
        carve::to_markdown(REPEATED),
        "*[A]: a\n\n*[A]: b\n\n<abbr title=\"b\">A</abbr> here.\n"
    );
    assert_eq!(carve::to_carve(REPEATED), "*[A]: a\n\n*[A]: b\n\nA here.\n");
}

// --- every occurrence, and every container ---------------------------------

/// "at every occurrence" is the clause's wording, and the terminal already
/// behaved this way. Corpus `177-two-abbreviation-definitions` pins two terms;
/// this pins one term appearing twice.
#[test]
fn the_expansion_lands_at_every_occurrence() {
    assert_eq!(
        carve::to_plain_text("*[HTML]: Long Form\n\nHTML and HTML again.\n"),
        "HTML (Long Form) and HTML (Long Form) again.\n"
    );
}

/// Corpus `305-an-abbreviation-expands-inside-an-inline-container`, which has no
/// non-HTML sidecar. An occurrence inside emphasis, inside a plain span and
/// inside a table cell counts the same as one in a bare paragraph, so a document
/// whose only occurrence sits in a container still loses the definition line.
#[test]
fn an_occurrence_inside_an_inline_container_counts() {
    let source = "*[HTML]: Long Form\n\nThe /HTML/ spec and [HTML]{.k} here.\n";
    assert_eq!(
        carve::to_plain_text(source),
        "The HTML (Long Form) spec and HTML (Long Form) here.\n"
    );
}

#[test]
fn an_occurrence_inside_a_table_cell_counts() {
    let source = "*[HTML]: Long Form\n\n| HTML |\n| ---- |\n| body |\n";
    assert_eq!(carve::to_plain_text(source), "HTML (Long Form)\nbody\n");
}

/// Corpus `95-abbreviation-definition-interrupts-a-paragraph`: the definition is
/// written BELOW its use. Source order does not enter the decision.
#[test]
fn a_definition_written_after_its_use_still_loses_its_line() {
    let source = "The HTML spec is long.\n*[HTML]: HyperText Markup Language\n";
    assert_eq!(
        carve::to_plain_text(source),
        "The HTML (HyperText Markup Language) spec is long.\n"
    );
    assert_eq!(
        carve::to_carve(source),
        "The HTML spec is long.\n\n*[HTML]: HyperText Markup Language\n"
    );
}

/// Corpus `150-abbreviation-title-escapes-its-markup-characters`. These targets
/// have no markup to escape into, so the expansion arrives as written - which is
/// worth pinning precisely because the Markdown and HTML targets do escape it,
/// and a shared escape would be the easy mistake.
#[test]
fn the_expansion_reaches_plain_and_the_terminal_unescaped() {
    let source = "*[HTML]: Hyper & Text < Markup > \"quoted\"\n\nThe HTML spec.\n";
    assert_eq!(
        carve::to_plain_text(source),
        "The HTML (Hyper & Text < Markup > \"quoted\") spec.\n"
    );
    assert_eq!(
        carve::to_ansi(source),
        format!(
            "The HTML{} spec.\n",
            dim(" (Hyper & Text < Markup > \"quoted\")")
        )
    );
    assert_eq!(
        carve::to_markdown(source),
        "*[HTML]: Hyper &amp; Text &lt; Markup &gt; \"quoted\"\n\n\
         The <abbr title=\"Hyper &amp; Text &lt; Markup &gt; &quot;quoted&quot;\">HTML</abbr> spec.\n"
    );
}

// --- positions whose subtree these two targets never render -----------------
//
// Both were found by `codex review` and both lost the expansion outright: the
// definition line went while the occurrence printed no expansion, so the string
// existed nowhere in the output. They are one rule, not two - the decision has
// to follow what the TARGET emits, and where it emits raw source instead of the
// subtree, the abbreviation below it is not an occurrence.

/// An unresolved reference link is emitted as its raw source, so an
/// abbreviation in its label expands nowhere and the definition keeps its line.
#[test]
fn an_occurrence_in_an_unresolved_reference_link_does_not_count() {
    let source = "*[HTML]: Long Form\n\n[HTML][missing]\n";
    assert_eq!(
        carve::to_plain_text(source),
        "*[HTML]: Long Form\n\n[HTML][missing]\n"
    );
    assert_eq!(
        carve::to_ansi(source),
        format!("{}\n\n[HTML][missing]\n", dim("*[HTML]: Long Form"))
    );
}

/// A citation group is emitted as its raw source on both targets, resolved or
/// not, so an abbreviation the Citations extension left in a suffix reaches
/// HTML and nothing else. The definition keeps its line.
#[test]
fn an_occurrence_inside_a_citation_part_does_not_count() {
    let citations = carve::Citations::new();
    let options = carve::Options::new().with_extension(&citations);
    let source = "*[HTML]: Long Form\n\n[@a, see HTML] here.\n\n[@a]: Entry A.\n";
    assert_eq!(
        carve::to_plain_text_with_options(source, &options),
        "*[HTML]: Long Form\n\n[@a, see HTML] here.\n"
    );
    assert_eq!(
        carve::to_ansi_with_options(source, &options),
        format!("{}\n\n[@a, see HTML] here.\n", dim("*[HTML]: Long Form"))
    );
    // The occurrence really does expand on HTML - which is what makes this a
    // divergence between targets rather than a term that never resolved.
    assert!(
        carve::to_html_with_options(source, &options)
            .contains("<abbr title=\"Long Form\">HTML</abbr>"),
        "the abbreviation must resolve, or this pins nothing"
    );
}

/// HTML is outside this clause entirely and drops the definition as it always
/// has - it has nowhere to put one.
#[test]
fn html_is_unaffected() {
    assert_eq!(
        carve::to_html(BASIC),
        "<p>The <abbr title=\"HyperText Markup Language\">HTML</abbr> spec is essential reading.</p>"
    );
}
