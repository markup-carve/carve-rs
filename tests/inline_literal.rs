//! Inline literal (`` `…`{!} ``, grammar PART 9 §27). A code span whose trailing
//! attribute block has `!` as its FIRST token: the verbatim content is
//! HTML-escaped and emitted by EVERY renderer (never dropped or target-routed),
//! with the `<code>` wrapper removed. Bare escaped text when the block carries no
//! further attribute; a `<span>` carrying them otherwise. Mirrors carve-js'
//! `test/inline-literal.test.ts`.

use carve::profile::canonical_inline_type;
use carve::{
    to_ansi, to_carve, to_html, to_html_with_options, to_markdown, to_plain_text, InlineNode,
    LiteralInline, Options, Profile,
};

fn h(src: &str) -> String {
    to_html(src)
}

// ---- HTML semantics ----

#[test]
fn bare_escaped_text_with_no_element_when_no_further_attribute() {
    assert_eq!(h("`/kaet/`{!}"), "<p>/kaet/</p>");
}

#[test]
fn span_carrying_a_class() {
    assert_eq!(
        h("`/kaet/`{! .ipa}"),
        "<p><span class=\"ipa\">/kaet/</span></p>"
    );
}

#[test]
fn span_carrying_class_and_id_in_source_order() {
    assert_eq!(
        h("`/kaet/`{! .ipa #cat}"),
        "<p><span class=\"ipa\" id=\"cat\">/kaet/</span></p>"
    );
}

#[test]
fn attributes_render_in_recorded_source_order() {
    assert_eq!(
        h("`x`{! .a #b k=v}"),
        "<p><span class=\"a\" id=\"b\" k=\"v\">x</span></p>"
    );
    // ... and the reverse source order flips the emitted order.
    assert_eq!(
        h("`x`{! k=v #b .a}"),
        "<p><span k=\"v\" id=\"b\" class=\"a\">x</span></p>"
    );
}

#[test]
fn html_escapes_the_content() {
    // The opposite of raw passthrough, which emits unescaped.
    assert_eq!(h("`a<b>`{!}"), "<p>a&lt;b&gt;</p>");
    assert_eq!(
        h("`&amp; <s>`{! .x}"),
        "<p><span class=\"x\">&amp;amp; &lt;s&gt;</span></p>"
    );
}

#[test]
fn no_inline_construct_is_recognized_inside() {
    assert_eq!(h("`*not bold*`{!}"), "<p>*not bold*</p>");
    assert_eq!(h("`[t](/u)`{!}"), "<p>[t](/u)</p>");
}

#[test]
fn flows_inline_within_a_paragraph() {
    assert_eq!(
        h("The word cat is `/kaet/`{!} in IPA."),
        "<p>The word cat is /kaet/ in IPA.</p>"
    );
}

#[test]
fn parses_to_a_literal_inline_node() {
    let doc = carve::parse("`/kaet/`{! .ipa}");
    let para = match &doc.children[0] {
        carve::BlockNode::Paragraph(p) => p,
        other => panic!("expected paragraph, got {other:?}"),
    };
    match &para.children[0] {
        InlineNode::LiteralInline(lit) => {
            assert_eq!(lit.content, "/kaet/");
            let attrs = lit.attrs.as_ref().expect("attrs present");
            assert_eq!(attrs.classes, vec!["ipa".to_string()]);
        }
        other => panic!("expected literal-inline, got {other:?}"),
    }
}

// ---- multibyte content ----

#[test]
fn handles_multibyte_ipa_content_without_panicking() {
    // Real phonemic transcription: U+02B0 (ʰ), U+00E6 (æ), U+02C8 (ˈ), U+02D0 (ː).
    assert_eq!(h("`/ˈkʰæːt/`{!}"), "<p>/ˈkʰæːt/</p>");
    assert_eq!(
        h("`/ˈkʰæːt/`{! .ipa}"),
        "<p><span class=\"ipa\">/ˈkʰæːt/</span></p>"
    );
    // fmt widening + round-trip must also stay on char boundaries.
    assert_eq!(
        to_carve("`/ˈkʰæːt/`{! .ipa}").trim_end(),
        "`/ˈkʰæːt/`{! .ipa}"
    );
}

// ---- smart typography is suppressed inside ----

#[test]
fn smart_typography_is_suppressed_inside() {
    assert_eq!(
        h("`a -- b ... \"q\" (c)`{!}"),
        "<p>a -- b ... \"q\" (c)</p>"
    );
    // Control: the same characters in ordinary text DO transform, proving the
    // suppression above is real, not an inert input.
    assert_eq!(h("a -- b ... \"q\" (c)"), "<p>a – b … “q” ©</p>");
    // Suppressed inside an attributed literal too.
    assert_eq!(
        h("`a -- b`{! .x}"),
        "<p><span class=\"x\">a -- b</span></p>"
    );
}

// ---- regression guards (unchanged constructs) ----

#[test]
fn generic_attributed_code_span_stays_a_code_element() {
    assert_eq!(h("`x`{.ipa}"), "<p><code class=\"ipa\">x</code></p>");
}

#[test]
fn raw_inline_passthrough_is_left_alone() {
    assert_eq!(h("`x`{=html}"), "<p>x</p>");
    // ... including its target-routed drop, which the literal never does.
    assert_eq!(h("`x`{=latex}"), "<p></p>");
}

#[test]
fn not_a_literal_when_the_sigil_is_not_the_first_token() {
    // `!` is not a valid attribute identifier, so the strict attribute rule
    // (§14) makes the whole block literal text.
    assert_eq!(h("`x`{.ipa !}"), "<p><code>x</code>{.ipa !}</p>");
}

#[test]
fn requires_whitespace_between_sigil_and_further_attribute() {
    assert_eq!(h("`x`{!.ipa}"), "<p><code>x</code>{!.ipa}</p>");
}

#[test]
fn inert_on_any_node_other_than_a_code_span() {
    assert_eq!(h("[t](/u){!}"), "<p><a href=\"/u\">t</a>{!}</p>");
    assert_eq!(h("*b*{!}"), "<p><strong>b</strong>{!}</p>");
}

// ---- chained standalone attribute blocks (carve-js parity) ----

#[test]
fn a_trailing_attribute_block_chains_onto_the_literal() {
    // A glued `{...}` after a literal merges like it does for a code span,
    // matching carve-js (its merge attaches to any non-text node).
    assert_eq!(h("`x`{! .a}{.b}"), "<p><span class=\"a b\">x</span></p>");
    // ... and it promotes a BARE literal to a span.
    assert_eq!(h("`x`{!}{.b}"), "<p><span class=\"b\">x</span></p>");
    // A space breaks the glue, so the second block stays literal text.
    assert_eq!(
        h("`x`{! .a} {.b}"),
        "<p><span class=\"a\">x</span> {.b}</p>"
    );
    // A second `{!…}` is not an attribute block (starts with `!`), so it does
    // not merge -- it stays literal text.
    assert_eq!(
        h("`x`{! .a}{! .c}"),
        "<p><span class=\"a\">x</span>{! .c}</p>"
    );
}

#[test]
fn an_invalid_literal_attr_payload_falls_back_to_a_code_span() {
    // `#1` is a digit-first (invalid) identifier, so `{! #1}` is not a literal;
    // the span reverts to a code span and the block renders as authored text
    // (where `#1` becomes a tag). Matches carve-js byte-for-byte, and the
    // fast-reject guard means many such openers stay O(n) overall.
    assert_eq!(
        h("`x`{! #1}"),
        "<p><code>x</code>{! <span class=\"tag\"><strong>#1</strong></span>}</p>"
    );
}

// ---- non-HTML renderers never drop it ----

#[test]
fn non_html_renderers_emit_the_content_as_literal_text() {
    let src = "`*not bold*`{!}";
    // Markdown escapes its own metacharacters so the text stays visible.
    assert_eq!(to_markdown(src).trim(), "\\*not bold\\*");
    assert_eq!(to_plain_text(src).trim(), "*not bold*");
    assert_eq!(to_ansi(src).trim(), "*not bold*");
}

#[test]
fn non_html_targets_keep_typography_verbatim() {
    let src = "`a -- b ... \"q\"`{!}";
    assert_eq!(to_markdown(src).trim(), "a -- b ... \"q\"");
    assert_eq!(to_plain_text(src).trim(), "a -- b ... \"q\"");
    assert_eq!(to_ansi(src).trim(), "a -- b ... \"q\"");
}

#[test]
fn carries_no_code_styling_in_ansi() {
    // A code span is colorized; the literal is prose, so it is not.
    assert_ne!(to_ansi("`x`").trim(), "x");
    assert_eq!(to_ansi("`x`{!}").trim(), "x");
}

// ---- contributes to heading text (slug) ----

#[test]
fn feeds_the_auto_heading_id_so_a_crossref_resolves() {
    // It renders as visible prose, so it must slug like a code span does.
    // Ids are case-preserving; the crossref folds case-insensitively.
    assert_eq!(
        h("# `Cat`{!}\n\nSee </#cat>"),
        "<section id=\"Cat\">\n  <h1>Cat</h1>\n  <p>See <a href=\"#Cat\">Cat</a></p>\n</section>"
    );
}

#[test]
fn slugs_exactly_like_the_equivalent_code_span() {
    let lit = h("# `Cat`{!}\n\nSee </#cat>");
    let code = h("# `Cat`\n\nSee </#cat>");
    assert_eq!(
        lit.replace("<code>", "").replace("</code>", ""),
        code.replace("<code>", "").replace("</code>", "")
    );
}

#[test]
fn combines_with_surrounding_heading_text() {
    assert!(h("# The `/kaet/`{!} sound").contains("id=\"The-kaet-sound\""));
}

// ---- carve serialization (fmt) ----

const FMT_CASES: &[&str] = &[
    "`/kaet/`{!}",
    "`/kaet/`{! .ipa}",
    "`/kaet/`{! .ipa #cat}",
    "`x`{! .a #b k=v}",
    "`a<b>`{!}",
    "`*not bold*`{!}",
    "`a -- b ... \"q\" (c)`{!}",
];

#[test]
fn fmt_round_trips_the_source_spelling() {
    for src in FMT_CASES {
        assert_eq!(to_carve(src).trim_end(), *src, "round-trip {src}");
    }
}

#[test]
fn fmt_widens_the_backtick_fence_when_content_contains_backticks() {
    assert_eq!(to_carve("``a`b``{!}").trim_end(), "``a`b``{!}");
    assert_eq!(to_carve("```a``b```{!}").trim_end(), "```a``b```{!}");
    // Content that starts/ends with a backtick gets the padding spaces back.
    assert_eq!(to_carve("`` `x` ``{!}").trim_end(), "`` `x` ``{!}");
}

#[test]
fn fmt_is_idempotent() {
    let mut cases: Vec<&str> = FMT_CASES.to_vec();
    cases.push("``a`b``{!}");
    cases.push("The word cat is `/kaet/`{!} in IPA");
    for src in cases {
        let once = to_carve(src);
        assert_eq!(to_carve(&once), once, "idempotent {src}");
    }
}

#[test]
fn fmt_preserves_the_to_html_invariant() {
    let mut cases: Vec<&str> = FMT_CASES.to_vec();
    cases.push("``a`b``{!}");
    cases.push("The word cat is `/kaet/`{!} in IPA");
    // The unchanged neighbours must keep the invariant too.
    cases.push("`x`{.ipa}");
    cases.push("`x`{.ipa !}");
    cases.push("[t](/u){!}");
    // Chained attribute blocks fold into one `{! …}` but must re-render the same.
    cases.push("`x`{! .a}{.b}");
    cases.push("`x`{!}{.b}");
    for src in cases {
        assert_eq!(to_html(&to_carve(src)), to_html(src), "invariant {src}");
    }
}

// ---- profiles ----

#[test]
fn classified_as_the_code_profile_type() {
    let node = InlineNode::LiteralInline(LiteralInline {
        content: "x".into(),
        attrs: None,
    });
    assert_eq!(canonical_inline_type(&node), Some("code"));
}

#[test]
fn allowed_wherever_a_code_span_is_allowed_across_all_presets() {
    let render = |src: &str, profile: Profile| {
        to_html_with_options(src, &Options::new().with_profile(profile))
    };
    for profile in [
        Profile::comment(),
        Profile::minimal(),
        Profile::article(),
        Profile::full(),
    ] {
        // code is in every preset's allowlist, so the literal rides along and
        // its attributes render exactly as an attributed code span's would.
        assert_eq!(
            render("`x`{! .ipa}", profile.clone()),
            "<p><span class=\"ipa\">x</span></p>"
        );
        assert_eq!(render("`x`{!}", profile.clone()), "<p>x</p>");
        // parity: the attributed code span it is a variant of is likewise allowed.
        assert_eq!(
            render("`x`{.ipa}", profile),
            "<p><code class=\"ipa\">x</code></p>"
        );
    }
}
