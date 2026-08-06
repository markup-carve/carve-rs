//! Footnote definitions print in source order on every target, not label order.
//!
//! §7 orders collected definitions by source position. `Document::footnote_defs`
//! is a `BTreeMap`, so iterating it yields LABEL order - which the `carve`
//! writer was fixed for in carve-rs#685 and the markdown, plain and ansi
//! renderers still had (carve-rs#686).
//!
//! `[^b]` written before `[^a]` is the discriminating shape: source order says
//! `[^b]` first, label order says `[^a]`, and nothing else separates the two
//! rules. The HTML renderer already numbers footnotes by FIRST USE, so the
//! endnote list a reader sees was in neither of the orders these targets
//! printed.

const SOURCE: &str = "see[^b] and[^a]\n\n[^b]: bee\n\n[^a]: ay\n";

fn position(haystack: &str, needle: &str) -> usize {
    haystack
        .find(needle)
        .unwrap_or_else(|| panic!("{needle:?} missing from:\n{haystack}"))
}

#[test]
fn markdown_prints_them_in_source_order() {
    let out = carve::to_markdown(SOURCE);
    assert!(
        position(&out, "[^b]: bee") < position(&out, "[^a]: ay"),
        "{out}"
    );
}

#[test]
fn plain_prints_them_in_source_order() {
    let out = carve::to_plain_text(SOURCE);
    assert!(position(&out, "bee") < position(&out, "ay"), "{out}");
}

#[test]
fn ansi_prints_them_in_source_order() {
    let out = carve::to_ansi(SOURCE);
    assert!(position(&out, "bee") < position(&out, "ay"), "{out}");
}

#[test]
fn the_html_endnote_list_is_in_the_same_order() {
    // The reference point: HTML numbers by first use, and `[^b]` is used first.
    let out = carve::to_html(SOURCE);
    assert!(position(&out, "bee") < position(&out, "ay"), "{out}");
}
