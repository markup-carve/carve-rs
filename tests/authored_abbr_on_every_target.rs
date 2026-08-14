//! An authored `abbr` outranks the document definition on EVERY target.
//!
//! carve#1127 ruled that a resolved abbreviation inside such a span contributes
//! only its visible text, and a renderer must not emit the nested expansion.
//! The HTML renderer honoured it; Markdown and ANSI emitted the DEFINITION's
//! text, and the plain target dropped the authored value entirely (carve#1176).
//!
//! Nothing caught it because `45-inline-extensions-11` had a `.html` fixture and
//! no `.md`, `.ansi` or `.txt` sidecar, so three of five targets were unpinned.

const WITH_DEFINITION: &str = "*[HTML]: Hyper Text Markup Language\n\n[HTML]{abbr=\"Custom\"}\n";

/// No definition line at all, so nothing but the span can carry the value.
const AUTHORED_ONLY: &str = "[HTML]{abbr=\"Custom\"} only.\n";

#[test]
fn the_authored_value_wins_on_html() {
    assert!(carve::to_html(WITH_DEFINITION).contains("<abbr title=\"Custom\">HTML</abbr>"));
}

#[test]
fn the_authored_value_wins_on_markdown() {
    // This emitted `title="Hyper Text Markup Language"` - the definition, taking
    // the override route carve#1127 forbids.
    let out = carve::to_markdown(WITH_DEFINITION);
    assert!(out.contains("<abbr title=\"Custom\">HTML</abbr>"), "{out}");
    assert!(!out.contains("Hyper Text Markup Language\">"), "{out}");
}

#[test]
fn the_authored_value_wins_on_ansi() {
    let out = carve::to_ansi(WITH_DEFINITION);
    assert!(out.contains("(Custom)"), "{out}");
    assert!(!out.contains("(Hyper Text Markup Language)"), "{out}");
}

/// PART 11 §10f drops the definition line on plain and the terminal ONLY where
/// the same output carries that definition's expansion. Here it does not: the
/// span outranks it, so the line is the only thing carrying "Hyper Text Markup
/// Language" and it stays. `45-inline-extensions-11` pins the same shape.
#[test]
fn the_outranked_definition_keeps_its_line() {
    assert_eq!(
        carve::to_plain_text(WITH_DEFINITION),
        "*[HTML]: Hyper Text Markup Language\n\nHTML (Custom)\n"
    );
    assert_eq!(
        carve::to_ansi(WITH_DEFINITION),
        "\u{1b}[2m*[HTML]: Hyper Text Markup Language\u{1b}[0m\n\nHTML\u{1b}[2m (Custom)\u{1b}[0m\n"
    );
}

/// The target the ticket did not name, and the worst of the three: the value
/// vanished with nothing else carrying it.
#[test]
fn the_plain_target_prints_an_authored_expansion() {
    assert!(
        carve::to_plain_text(AUTHORED_ONLY).contains("HTML (Custom) only."),
        "{}",
        carve::to_plain_text(AUTHORED_ONLY)
    );
}

/// The asymmetry this file used to assert is GONE, and its replacement is the
/// same test inverted.
///
/// It read: an AUTOMATIC expansion needs no parenthetical here, because the
/// `*[TERM]: expansion` line is emitted verbatim and carries the mapping once at
/// the definition. PART 11 §10f takes that line away on this target, so the
/// ground the exception stood on went with it and the expansion has to arrive
/// instead - emitting neither loses the author's text outright.
#[test]
fn the_plain_target_prints_an_automatic_expansion_instead_of_the_line() {
    assert_eq!(
        carve::to_plain_text("*[HTML]: Long Form\n\nThe HTML key.\n"),
        "The HTML (Long Form) key.\n"
    );
}

/// `{abbr=""}` is the spelling for "mark this, expand nothing" - HTML emits a
/// bare `<abbr>`. Collapsing it into the non-empty case would take a
/// distinction away from the author.
#[test]
fn an_empty_authored_abbr_prints_no_expansion() {
    assert_eq!(carve::to_plain_text("[HTML]{abbr=\"\"}\n"), "HTML\n");
    assert!(carve::to_html("[HTML]{abbr=\"\"}\n").contains("<abbr>HTML</abbr>"));
}

/// And it silences the DEFINITION's expansion too, which only became observable
/// on this target once §10f gave the automatic case a parenthetical of its own.
///
/// The span is authoritative (PART 9 §9), so the definition's expansion reaches
/// no target here - which is why the definition keeps its line, and why the
/// output is not `HTML (Long Form)`.
#[test]
fn an_empty_authored_abbr_silences_the_definition_on_plain() {
    assert_eq!(
        carve::to_plain_text("*[HTML]: Long Form\n\n[HTML]{abbr=\"\"} here.\n"),
        "*[HTML]: Long Form\n\nHTML here.\n"
    );
}
