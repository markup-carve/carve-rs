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

/// The asymmetry, stated as a test so it cannot be "simplified" away.
///
/// An AUTOMATIC expansion needs no parenthetical here: the `*[TERM]: expansion`
/// definition line is emitted verbatim, so the mapping survives once at the
/// definition rather than at every occurrence.
#[test]
fn the_plain_target_leaves_an_automatic_expansion_alone() {
    let out = carve::to_plain_text("*[HTML]: Long Form\n\nThe HTML key.\n");
    assert!(out.contains("*[HTML]: Long Form"), "{out}");
    assert!(out.contains("The HTML key."), "{out}");
    assert!(!out.contains("(Long Form)"), "{out}");
}

/// `{abbr=""}` is the spelling for "mark this, expand nothing" - HTML emits a
/// bare `<abbr>`. Collapsing it into the non-empty case would take a
/// distinction away from the author.
#[test]
fn an_empty_authored_abbr_prints_no_expansion() {
    assert_eq!(carve::to_plain_text("[HTML]{abbr=\"\"}\n"), "HTML\n");
    assert!(carve::to_html("[HTML]{abbr=\"\"}\n").contains("<abbr>HTML</abbr>"));
}
