//! The Markdown target has to emit a heading id whenever a cross-reference
//! resolves to it, or the link it also emits points at nothing.
//!
//! Both halves were broken in ways that hid each other. The renderer re-slugged
//! every heading instead of using the id the core assigned, so it never knew
//! about a disambiguated `-2` id; the reference to it then matched no known
//! heading and `render_link` degraded it to bare text, which looked like a
//! deliberate choice rather than a missing id (carve#352).

fn md(src: &str) -> String {
    carve::to_markdown(src)
}

#[test]
fn a_duplicate_heading_keeps_the_disambiguated_id_it_is_referenced_by() {
    // Two headings read `Setup`, so the second one's id is `Setup-2`. Deriving
    // the slug alone gave both `Setup` and lost the reference entirely.
    let out = md("## Setup\n\n## Setup\n\nSee </#setup-2>.\n");

    assert!(out.contains("## Setup {#Setup-2}"), "{out}");
    assert!(out.contains("[Setup](#Setup-2)"), "{out}");
}

#[test]
fn an_undisambiguated_heading_gains_no_suffix() {
    // Only a REFERENCED heading gets the suffix; the first `Setup` is not
    // referenced and stays clean.
    let out = md("## Setup\n\n## Setup\n\nSee </#setup-2>.\n");

    assert!(out.contains("## Setup\n"), "{out}");
    assert_eq!(out.matches("{#").count(), 1, "{out}");
}

#[test]
fn a_heading_referenced_only_from_a_footnote_body_still_gets_its_id() {
    let out = md("# H\n\nBody[^n]\n\n[^n]: see </#h>\n");

    assert!(out.contains("# H {#H}"), "{out}");
    assert!(out.contains("[H](#H)"), "{out}");
}

#[test]
fn a_self_referencing_heading_does_not_slug_its_own_expansion() {
    // `</#a>` resolves to a link carrying the heading's own text, so counting it
    // in the slug produced `A-A` and every id derived here disagreed with the
    // one the core assigned before resolution.
    let out = md("# A </#a>\n");

    assert!(out.contains("{#A}"), "{out}");
    assert!(!out.contains("A-A"), "{out}");
}
