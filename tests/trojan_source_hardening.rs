//! Trojan-Source hardening (corpus 117/118) plus the cross-reference id and
//! footnote-in-container behaviors pinned in the batch-conformance spec round.

// --- Heading ids: NFC + strip invisible controls (corpus 117) -------------

#[test]
fn heading_id_is_nfc_normalized() {
    // Decomposed `e` + combining acute (U+0301) and precomposed `é` (U+00E9)
    // must produce the SAME id. The rendered `<h1>` keeps the source bytes; only
    // the id is normalized.
    let decomposed = carve::to_html("# Cafe\u{0301}");
    let precomposed = carve::to_html("# Caf\u{00e9}");
    assert!(
        decomposed.contains("id=\"Caf\u{00e9}\""),
        "decomposed id not NFC-composed: {decomposed}"
    );
    assert!(
        precomposed.contains("id=\"Caf\u{00e9}\""),
        "precomposed id changed: {precomposed}"
    );
    // The heading text keeps the original (decomposed) code points.
    assert!(decomposed.contains("<h1>Cafe\u{0301}</h1>"), "{decomposed}");
}

#[test]
fn heading_id_strips_bidi_and_zero_width_controls() {
    // `A` + U+202E (bidi override) + `B` + U+200B (zero-width space) + `C`.
    let html = carve::to_html("# A\u{202e}B\u{200b}C");
    assert!(html.contains("id=\"ABC\""), "id not stripped: {html}");
    // The bidi override is stripped from the heading text too (corpus 118), but
    // the zero-width space survives in text (only ids drop it).
    assert!(
        html.contains("<h1>AB\u{200b}C</h1>"),
        "heading text wrong: {html}"
    );
}

#[test]
fn heading_id_strips_all_listed_zero_width_chars() {
    for zw in [
        '\u{200B}', '\u{200C}', '\u{200D}', '\u{2060}', '\u{FEFF}', '\u{00AD}',
    ] {
        let src = format!("# A{zw}B");
        let html = carve::to_html(&src);
        assert!(
            html.contains("id=\"AB\""),
            "zero-width U+{:04X} not stripped from id: {html}",
            zw as u32
        );
    }
}

// --- Rendered text + code: strip bidi-override controls (corpus 118) -------

#[test]
fn text_strips_bidi_override_controls() {
    assert_eq!(carve::to_html("a\u{202e}b"), "<p>ab</p>");
    // All of U+202A..U+202E and U+2066..U+2069 are removed from text.
    for c in [
        '\u{202A}', '\u{202B}', '\u{202C}', '\u{202D}', '\u{2066}', '\u{2069}',
    ] {
        let html = carve::to_html(&format!("x{c}y"));
        assert_eq!(
            html, "<p>xy</p>",
            "control U+{:04X} survived text",
            c as u32
        );
    }
}

#[test]
fn code_span_strips_bidi_override_controls() {
    assert_eq!(carve::to_html("`a\u{202e}b`"), "<p><code>ab</code></p>");
}

#[test]
fn code_block_strips_bidi_override_controls() {
    let html = carve::to_html("```\na\u{202e}b\n```");
    assert!(
        html.contains("<code>ab\n</code>") || html.contains(">ab"),
        "{html}"
    );
    assert!(
        !html.contains('\u{202e}'),
        "bidi control leaked into code block: {html}"
    );
}

#[test]
fn text_keeps_directional_marks_and_zero_width() {
    // LRM / RLM and zero-width chars are NOT stripped from text (only the
    // overrides/isolates are, and zero-width only from ids).
    assert_eq!(carve::to_html("a\u{200e}b"), "<p>a\u{200e}b</p>");
    assert_eq!(carve::to_html("a\u{200b}b"), "<p>a\u{200b}b</p>");
}

// --- Cross-reference id derivation (corpus 116) ---------------------------

#[test]
fn self_cross_reference_does_not_pollute_heading_id() {
    // `# A </#a>` resolves to itself; the id stays `A` (the cross-reference's
    // resolved-link text must not feed the slug, which would give `A-A`).
    let html = carve::to_html("# A </#a>");
    assert!(html.contains("id=\"A\""), "id polluted by crossref: {html}");
    assert!(html.contains("href=\"#A\""), "{html}");
}

// --- Footnote definition inside a container (corpus 115) -------------------

#[test]
fn footnote_def_inside_blockquote_is_collected() {
    let html = carve::to_html("See [^a].\n\n> [^a]: note body\n");
    assert!(
        html.contains("role=\"doc-endnotes\""),
        "no endnotes: {html}"
    );
    assert!(html.contains("note body"), "body missing: {html}");
    assert!(html.contains("href=\"#fn1\""), "ref not resolved: {html}");
}

#[test]
fn footnote_def_inside_list_item_is_collected() {
    let html = carve::to_html("See [^a].\n\n- [^a]: note body\n");
    assert!(
        html.contains("role=\"doc-endnotes\""),
        "no endnotes: {html}"
    );
    assert!(html.contains("note body"), "body missing: {html}");
    // The list still renders (empty item).
    assert!(html.contains("<ul>"), "list dropped: {html}");
}

#[test]
fn footnote_looking_line_in_fenced_code_is_literal() {
    // A `[^x]: ...` line inside a fenced code block is literal content, NOT a
    // footnote definition -- at the top level and inside a blockquote (the
    // container-prefix strip must not expose fenced lines as definitions).
    let top = carve::to_html("```\n[^x]: code\n```\n");
    assert!(
        top.contains("[^x]: code"),
        "top-level fenced def leaked: {top}"
    );
    assert!(
        !top.contains("doc-endnotes"),
        "should not be an endnote: {top}"
    );

    let bq = carve::to_html("> ```\n> [^x]: code\n> ```\n");
    assert!(
        bq.contains("[^x]: code"),
        "blockquote fenced def leaked: {bq}"
    );
    assert!(
        !bq.contains("doc-endnotes"),
        "should not be an endnote: {bq}"
    );
}
