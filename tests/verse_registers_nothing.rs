//! A line block's body is verse: nothing inside one is claimed (PART 9 §23).
//!
//! The definition prepass kept a definition-shaped verse line - that was
//! carve-rs#491, and it fixed the content loss - but went on REGISTERING it,
//! deliberately, because whether any engine should was still open (carve#557).
//! carve#574 answered it: the line renders and defines nothing.

fn html(source: &str) -> String {
    carve::to_html(source)
        .replace('\n', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn a_definition_in_verse_does_not_resolve_elsewhere() {
    let out = html("::: |\n[d]: /u\n:::\n\nsee [d][]\n");

    assert!(out.contains("[d]: /u"), "the verse line vanished: {out}");
    assert!(
        !out.contains("href=\"/u\""),
        "the definition registered: {out}"
    );
}

#[test]
fn a_definition_after_the_verse_still_resolves() {
    let out = html("::: |\nverse\n:::\n\n[d]: /u\n\nsee [d][]\n");

    assert!(out.contains("href=\"/u\""), "{out}");
}

#[test]
fn a_wider_verse_fence_closes_on_its_own_width() {
    let out = html(":::: |\n[d]: /u\n:::\nstill verse\n::::\n\nsee [d][]\n");

    assert!(!out.contains("href=\"/u\""), "{out}");
}

#[test]
fn a_footnote_definition_in_verse_stays_literal() {
    let out = html("::: |\n[^f]: t\n:::\n");

    assert!(out.contains("[^f]: t"), "{out}");
    assert!(!out.contains("doc-endnotes"), "{out}");
}
