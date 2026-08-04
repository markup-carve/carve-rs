//! PART 10 §10a: an unused definition survives the non-HTML targets.
//!
//! "HTML drops it, because HTML has nowhere to put a definition nobody used;
//! the other three do not get to drop content the author wrote." Dropping it
//! also makes the output depend on whether a reference exists elsewhere, so
//! adding one reference changes an unrelated line.
//!
//! Two of the three constructs are here. A LINK reference definition is not: it
//! leaves no node in the tree at all, so no renderer can reach it, and giving it
//! one is a vocabulary change (carve#592).

fn strip_ansi(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            for skip in chars.by_ref() {
                if skip == 'm' {
                    break;
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

#[test]
fn an_abbreviation_definition_survives_markdown() {
    assert!(carve::to_markdown("*[AB]: expansion\n").contains("*[AB]: expansion"));
}

#[test]
fn an_abbreviation_definition_survives_plain() {
    assert!(carve::to_plain_text("*[AB]: expansion\n").contains("*[AB]: expansion"));
}

#[test]
fn an_abbreviation_definition_survives_ansi() {
    let out = strip_ansi(&carve::to_ansi("*[AB]: expansion\n"));
    assert!(out.contains("*[AB]: expansion"), "{out}");
}

#[test]
fn an_abbreviation_definition_survives_beside_its_use() {
    // The output must not depend on whether a reference exists: the definition
    // is emitted either way.
    let out = carve::to_markdown("*[AB]: expansion\n\nAB here.\n");
    assert!(out.contains("*[AB]: expansion"), "{out}");
}

#[test]
fn a_footnote_definition_keeps_its_caret_on_plain() {
    let out = carve::to_plain_text("[^n]: a note\n");
    assert!(out.contains("[^n]:"), "{out}");
}

#[test]
fn a_footnote_definition_keeps_its_caret_on_ansi() {
    let out = strip_ansi(&carve::to_ansi("[^n]: a note\n"));
    assert!(out.contains("[^n]"), "{out}");
}

#[test]
fn a_footnote_definition_keeps_its_caret_on_markdown() {
    assert!(carve::to_markdown("[^n]: a note\n").contains("[^n]: a note"));
}

#[test]
fn html_still_drops_an_unused_definition() {
    // The rule is about the OTHER three targets; HTML has nowhere to put one.
    let out = carve::to_html("*[AB]: expansion\n");
    assert!(!out.contains("expansion"), "{out}");
}
