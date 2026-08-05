//! R1's heading index folds NFC, and not NFKC (carve#725).
//!
//! Heading IDS are NFC-normalized (section 25). The heading-TEXT index was not,
//! and this engine still looked right - but only through `resolve_ref`'s slug
//! fallback, which answers a cross-spelling reference solely when the heading's
//! id IS the slug of its text. That made the behavior an accident rather than a
//! rule, and the accident is observable: give the heading an id of its own and
//! the reference stopped resolving here while the executable spec, carve-js and
//! carve-php all resolved it.
//!
//! So the load-bearing case in this file is the CUSTOM-ID one. The plain case
//! passed before the fold moved onto the text index, which is exactly why it
//! could not be the test.
//!
//! NFC and not NFKC: compatibility folding would change which text the author is
//! quoting rather than how it is spelled.

/// `e` + U+0301 COMBINING ACUTE.
const DECOMPOSED: &str = "Cafe\u{0301}";

/// Precomposed U+00E9.
const PRECOMPOSED: &str = "Caf\u{00E9}";

fn html(source: &str) -> String {
    carve::to_html(source)
}

#[test]
fn a_precomposed_reference_resolves_a_decomposed_heading() {
    let out = html(&format!("# {DECOMPOSED}\n\nsee [{PRECOMPOSED}][]\n"));
    assert!(
        out.contains(&format!("<a href=\"#{PRECOMPOSED}\"")),
        "{out}"
    );
    // The id side was already NFC; this asserts the lookup uses the same
    // alphabet.
    assert!(out.contains(&format!("id=\"{PRECOMPOSED}\"")), "{out}");
}

#[test]
fn a_decomposed_reference_resolves_a_precomposed_heading() {
    let out = html(&format!("# {PRECOMPOSED}\n\nsee [{DECOMPOSED}][]\n"));
    assert!(
        out.contains(&format!("<a href=\"#{PRECOMPOSED}\"")),
        "{out}"
    );
}

#[test]
fn the_fold_is_on_the_text_index_not_the_slug_fallback() {
    // The heading carries its own id, so `resolve_ref`'s slug fallback cannot
    // answer: the slug of the label is `Café`, and no such id exists. Only a
    // normalized TEXT index resolves this, which is what R1 describes.
    //
    // This is the case that failed here while three other readers resolved it.
    let out = html(&format!(
        "{{#custom}}\n# {DECOMPOSED}\n\nsee [{PRECOMPOSED}][]\n"
    ));
    assert!(out.contains("<a href=\"#custom\""), "{out}");
}

#[test]
fn the_same_spelling_cases_still_resolve_with_a_custom_id() {
    // Control for the case above: without the fold this one passed, so if it
    // ever fails the change broke the index rather than the normalization.
    let out = html(&format!(
        "{{#custom}}\n# {DECOMPOSED}\n\nsee [{DECOMPOSED}][]\n"
    ));
    assert!(out.contains("<a href=\"#custom\""), "{out}");
}

#[test]
fn the_heading_text_keeps_the_author_s_spelling() {
    // Normalization is for MATCHING. The rendered heading keeps its own bytes.
    let out = html(&format!("# {DECOMPOSED}\n\nsee [{PRECOMPOSED}][]\n"));
    assert!(out.contains(&format!("<h1>{DECOMPOSED}</h1>")), "{out}");
}

#[test]
fn case_and_whitespace_folding_still_apply() {
    let out = html("# Getting  Started\n\nsee [getting started][]\n");
    assert!(out.contains("<a href=\"#Getting-Started\""), "{out}");
}

#[test]
fn compatibility_equivalence_is_not_folded() {
    // Each of these resolves under NFKC. It stays out: a fix reaching for
    // compatibility normalization - or for the ASCII transliteration this crate
    // uses for ids - changes which text is being quoted.
    for (heading, reference) in [
        ("\u{FB01}le", "file"),     // U+FB01 LATIN SMALL LIGATURE FI
        ("\u{2460} one", "1 one"),  // U+2460 CIRCLED DIGIT ONE
        ("\u{FF41}\u{FF42}", "ab"), // U+FF41/U+FF42 FULLWIDTH A, B
    ] {
        let out = html(&format!("# {heading}\n\nsee [{reference}][]\n"));
        assert!(
            out.contains(&format!("[{reference}][]")),
            "{reference} should not reach {heading}: {out}"
        );
    }
}

#[test]
fn the_crossref_form_is_unaffected() {
    // `</#id>` addresses an id, not heading text, and keeps its own rule (R4).
    let out = html(&format!("# {DECOMPOSED}\n\nsee </#{PRECOMPOSED}>\n"));
    assert!(
        out.contains(&format!("<a href=\"#{PRECOMPOSED}\"")),
        "{out}"
    );
}
