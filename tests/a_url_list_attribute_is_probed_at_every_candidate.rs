//! PART 9 §25: a URL-list attribute is probed at every candidate, not at its
//! head (`markup-carve/carve#1320`).
//!
//! The clause's value probe reads the LEADING scheme, which vouches for the
//! whole value only where the whole value is one URL. Four attributes carry a
//! list of them - `srcset`, `imagesrcset`, `ping` and `attributionsrc` - so a
//! dangerous scheme in any position but the first went unread, and the same
//! value got one answer in position one and another in position two.
//!
//! THIS FILE IS THE RENDERER'S HALF. `markup-carve/carve-rs#1065` refuses
//! `srcset` on the HTML IMPORT path, which is the importer declining to admit
//! an attribute; it says nothing about a value an author wrote by hand in a
//! `.crv` document, which is what these cases exercise.
//!
//! EVERY CASE IS ASSERTED IN BOTH DIRECTIONS, per attribute. A refusal-only
//! suite invites the next person to widen the rule until it fires on prose,
//! and a `title` that carries a colon is ordinary English. `ping` matters most
//! of the four: a browser really does POST to those URLs.

fn h(src: &str) -> String {
    carve::to_html(src).trim().to_string()
}

// ---------------------------------------------------------------------------
// srcset - commas AND ASCII whitespace separate candidates.
// ---------------------------------------------------------------------------

#[test]
fn srcset_blanks_a_dangerous_scheme_in_a_non_leading_candidate() {
    assert_eq!(
        h(r#"![a](safe.png){srcset="safe.png 1x, javascript:alert(1) 2x"}"#),
        r#"<img src="safe.png" alt="a" srcset="">"#
    );
}

#[test]
fn srcset_still_blanks_the_leading_candidate_it_always_did() {
    assert_eq!(
        h(r#"![a](safe.png){srcset="javascript:alert(1) 1x, safe.png 2x"}"#),
        r#"<img src="safe.png" alt="a" srcset="">"#
    );
}

/// THE COMMA IS A SEPARATOR FOR THIS HALF OF THE SET, and this is the case that
/// proves it: with no space after the comma, a whitespace-only split reads
/// `1x,javascript:alert(1)` as one token whose leading scheme is `1x`, and the
/// payload hides inside the first candidate's descriptor.
#[test]
fn srcset_splits_on_a_comma_with_no_space_after_it() {
    assert_eq!(
        h(r#"![a](safe.png){srcset="safe.png 1x,javascript:alert(1) 2x"}"#),
        r#"<img src="safe.png" alt="a" srcset="">"#
    );
}

/// THE DELIBERATE OVER-BLANK, PINNED SO NOBODY "FIXES" IT. A srcset URL may
/// itself contain a comma, so this is ONE candidate to a consumer and is
/// blanked anyway. Reading it exactly would mean requiring the HTML
/// candidate-list algorithm from three engines that must agree byte for byte.
/// An implementation that renders this verbatim has diverged from the spec.
#[test]
fn srcset_over_blanks_a_comma_inside_one_candidate_and_that_is_the_chosen_side() {
    assert_eq!(
        h(r#"![a](safe.png){srcset="https://example.com/a,data:x 1x"}"#),
        r#"<img src="safe.png" alt="a" srcset="">"#
    );
}

#[test]
fn srcset_keeps_an_ordinary_candidate_list() {
    assert_eq!(
        h(r#"![a](safe.png){srcset="small.png 1x, large.png 2x"}"#),
        r#"<img src="safe.png" alt="a" srcset="small.png 1x, large.png 2x">"#
    );
}

/// THE NAME MATCH IS CASE-INSENSITIVE, like the `on` prefix in the same
/// clause. The element still carries the author's spelling, so matching the
/// exact bytes would leave `SRCSET` unprobed while echoing it back.
#[test]
fn srcset_is_matched_case_insensitively_and_keeps_the_authors_spelling() {
    assert_eq!(
        h(r#"![a](safe.png){SRCSET="safe.png 1x, javascript:alert(1) 2x"}"#),
        r#"<img src="safe.png" alt="a" SRCSET="">"#
    );
    assert_eq!(
        h(r#"![a](safe.png){SrcSet="small.png 1x"}"#),
        r#"<img src="safe.png" alt="a" SrcSet="small.png 1x">"#
    );
}

// ---------------------------------------------------------------------------
// imagesrcset - same grammar as srcset, its own name.
// ---------------------------------------------------------------------------

#[test]
fn imagesrcset_blanks_a_dangerous_scheme_in_a_non_leading_candidate() {
    assert_eq!(
        h(r#"![a](safe.png){imagesrcset="safe.png 1x, javascript:alert(1) 2x"}"#),
        r#"<img src="safe.png" alt="a" imagesrcset="">"#
    );
}

#[test]
fn imagesrcset_splits_on_a_comma_with_no_space_after_it() {
    assert_eq!(
        h(r#"![a](safe.png){imagesrcset="safe.png 1x,vbscript:x 2x"}"#),
        r#"<img src="safe.png" alt="a" imagesrcset="">"#
    );
}

#[test]
fn imagesrcset_keeps_an_ordinary_candidate_list() {
    assert_eq!(
        h(r#"![a](safe.png){imagesrcset="small.png 1x, large.png 2x"}"#),
        r#"<img src="safe.png" alt="a" imagesrcset="small.png 1x, large.png 2x">"#
    );
}

// ---------------------------------------------------------------------------
// ping - ASCII WHITESPACE ONLY. The one a user agent actually fetches.
// ---------------------------------------------------------------------------

#[test]
fn ping_blanks_a_dangerous_scheme_in_a_non_leading_url() {
    assert_eq!(
        h(r#"[y](safe.html){ping="safe.html javascript:alert(1)"}"#),
        r#"<p><a href="safe.html" ping="">y</a></p>"#
    );
}

#[test]
fn ping_still_blanks_the_leading_url_it_always_did() {
    assert_eq!(
        h(r#"[y](safe.html){ping="javascript:alert(1) safe.html"}"#),
        r#"<p><a href="safe.html" ping="">y</a></p>"#
    );
}

/// THE BINDING FALSE-POSITIVE BOUND, and the reason the two halves of the set
/// do not share a separator rule. `ping`'s grammar holds no comma at all, so a
/// comma here is part of a single URL's path. Splitting on it would blank a
/// legitimate value, and the first time that fired in the wild somebody would
/// loosen the whole rule to stop it.
#[test]
fn ping_does_not_split_on_a_comma_inside_one_url() {
    assert_eq!(
        h(r#"[y](safe.html){ping="https://example.com/a,data:x"}"#),
        r#"<p><a href="safe.html" ping="https://example.com/a,data:x">y</a></p>"#
    );
}

#[test]
fn ping_keeps_an_ordinary_url_set() {
    assert_eq!(
        h(r#"[y](safe.html){ping="https://a.example/p https://b.example/p"}"#),
        r#"<p><a href="safe.html" ping="https://a.example/p https://b.example/p">y</a></p>"#
    );
}

// ---------------------------------------------------------------------------
// attributionsrc - ASCII whitespace only, same as `ping`.
// ---------------------------------------------------------------------------

#[test]
fn attributionsrc_blanks_a_dangerous_scheme_in_a_non_leading_url() {
    assert_eq!(
        h(r#"[y](safe.html){attributionsrc="https://example.com/s javascript:alert(1)"}"#),
        r#"<p><a href="safe.html" attributionsrc="">y</a></p>"#
    );
}

#[test]
fn attributionsrc_does_not_split_on_a_comma_inside_one_url() {
    assert_eq!(
        h(r#"[y](safe.html){attributionsrc="https://example.com/a,data:x"}"#),
        r#"<p><a href="safe.html" attributionsrc="https://example.com/a,data:x">y</a></p>"#
    );
}

#[test]
fn attributionsrc_keeps_an_ordinary_url_set() {
    assert_eq!(
        h(r#"[y](safe.html){attributionsrc="https://a.example/s https://b.example/s"}"#),
        r#"<p><a href="safe.html" attributionsrc="https://a.example/s https://b.example/s">y</a></p>"#
    );
}

// ---------------------------------------------------------------------------
// The other direction: prose attributes are NOT in the set and must not be
// tokenized. This is the half that stops the rule creeping outward.
// ---------------------------------------------------------------------------

#[test]
fn a_prose_attribute_carrying_a_colon_is_not_tokenized() {
    assert_eq!(
        h(r#"[z](safe.html){title="See: RFC 3986, http://example.com"}"#),
        r#"<p><a href="safe.html" title="See: RFC 3986, http://example.com">z</a></p>"#
    );
    assert_eq!(
        h(r#"[z](safe.html){aria-label="Step 2: open data:sets"}"#),
        r#"<p><a href="safe.html" aria-label="Step 2: open data:sets">z</a></p>"#
    );
}

/// A NON-LIST URL ATTRIBUTE KEEPS THE LEADING RULE EXACTLY AS IT WAS.
/// `background` is one URL, so a colon-bearing token after the first is part
/// of that URL and not a second candidate.
#[test]
fn a_single_url_attribute_keeps_the_leading_scheme_rule() {
    assert_eq!(
        h(r#"[z](safe.html){background="ok.png data:x"}"#),
        r#"<p><a href="safe.html" background="ok.png data:x">z</a></p>"#
    );
    // ... and its head probe still strips a split scheme, which is the
    // behavior the token rule replaces rather than joins for the four names.
    assert_eq!(
        h("[z](safe.html){background=\"java\tscript:alert(1)\"}"),
        r#"<p><a href="safe.html" background="">z</a></p>"#
    );
}

/// THE TOKEN PASS IS ADDED TO THE VALUE-WIDE PROBE, NOT SWAPPED FOR IT, AND
/// THIS ROW IS THE ONLY THING THAT SAYS SO. The clause changes WHERE the probe
/// runs, not WHAT it denies, so the head probe stays.
///
/// Split on ASCII whitespace, `java script:alert(1)` is two clean tokens and a
/// token-ONLY engine emits it verbatim - denying LESS than this engine denied
/// before the ruling, which would be a regression shipped as a security fix.
/// The value-wide probe's strip closes exactly the gap the whitespace split
/// opens.
///
/// THE CORPUS CANNOT CATCH THIS. A token-only engine passes all 1141 documents
/// including the ten that pin the ruling, so without this row three engines
/// could diverge on a security boundary with a green conformance suite. Pinned
/// the same way in `markup-carve/carve-js#1164`.
#[test]
fn a_scheme_split_across_a_separator_is_still_denied_by_the_value_wide_probe() {
    assert_eq!(
        h(r#"[y](safe.html){ping="java script:alert(1)"}"#),
        r#"<p><a href="safe.html" ping="">y</a></p>"#
    );
    assert_eq!(
        h(r#"![a](safe.png){srcset="java script:alert(1) 1x"}"#),
        r#"<img src="safe.png" alt="a" srcset="">"#
    );
}

// ---------------------------------------------------------------------------
// Composition with the strip. The clause's earlier paragraphs are not bypassed
// by tokenizing: the strip runs PER TOKEN rather than once at the front.
// ---------------------------------------------------------------------------

/// U+202F NARROW NO-BREAK SPACE leads a non-leading candidate. It is stripped
/// before the scheme is read, so the candidate blanks wherever it sits.
#[test]
fn the_strip_runs_per_token_not_once_at_the_front() {
    assert_eq!(
        h("[y](safe.html){ping=\"safe.html \u{202F}javascript:alert(1)\"}"),
        r#"<p><a href="safe.html" ping="">y</a></p>"#
    );
}

/// THE `Cf` DECISION COMPOSES UNCHANGED (`markup-carve/carve#782`). A token
/// beginning U+200B fails WHATWG URL parsing and lands inert, so it is left
/// alone at EVERY position, exactly as it is at the head.
#[test]
fn a_zero_width_space_before_the_scheme_is_left_alone_at_every_position() {
    assert_eq!(
        h("[y](safe.html){ping=\"safe.html \u{200B}javascript:alert(1)\"}"),
        "<p><a href=\"safe.html\" ping=\"safe.html \u{200B}javascript:alert(1)\">y</a></p>"
    );
}

/// THE SPLIT IS NARROWER THAN THE STRIP, DELIBERATELY. Both grammars put their
/// boundaries at ASCII whitespace, so `a<U+202F>javascript:x` is ONE token to a
/// consumer and resolves as a relative URL - the strip then reads its scheme as
/// `ajavascript`, which is not denylisted.
#[test]
fn a_unicode_space_inside_a_token_does_not_split_it() {
    assert_eq!(
        h("[y](safe.html){ping=\"a\u{202F}javascript:x\"}"),
        "<p><a href=\"safe.html\" ping=\"a\u{202F}javascript:x\">y</a></p>"
    );
}

// ---------------------------------------------------------------------------
// What is blanked, and where the rule reaches.
// ---------------------------------------------------------------------------

/// THE WHOLE VALUE, NOT THE OFFENDING CANDIDATE. Excising one would make the
/// rendered attribute differ from the author's bytes, and would give one value
/// a third outcome when the defect being fixed is that it already had two.
#[test]
fn the_whole_value_is_blanked_and_not_the_offending_candidate() {
    let out = h(r#"![a](safe.png){srcset="one.png 1x, javascript:alert(1) 2x, three.png 3x"}"#);
    assert_eq!(out, r#"<img src="safe.png" alt="a" srcset="">"#);
    assert!(
        !out.contains("one.png") && !out.contains("three.png"),
        "a surviving candidate means the value was rewritten rather than blanked: {out}"
    );
}

/// THE RULE IS ON THE NAME, NOT ON THE ELEMENT. The clause closes its set by a
/// criterion - a value grammar that is a list of URLs a consumer fetches - and
/// an attribute block can put any name on any element, so probing only where
/// the HTML Standard allows the attribute would leave the rest unread.
#[test]
fn the_set_is_matched_on_the_name_wherever_the_attribute_lands() {
    assert_eq!(
        h(r#"[x]{ping="ok.html javascript:alert(1)"}"#),
        r#"<p><span ping="">x</span></p>"#
    );
    assert_eq!(
        h("{srcset=\"a.png 1x, javascript:alert(1) 2x\"}\npara\n"),
        r#"<p srcset="">para</p>"#
    );
}

/// EVERY DENYLISTED SCHEME, NOT JUST `javascript:`. The token probe is the same
/// function the head probe calls, so the two cannot deny different sets - that
/// is the whole point of routing both through it.
#[test]
fn the_token_probe_denies_the_same_schemes_the_head_probe_does() {
    for scheme in ["javascript", "vbscript", "data", "file", "ms-msdt", "jar"] {
        let src = format!(r#"[y](safe.html){{ping="safe.html {scheme}:payload"}}"#);
        assert_eq!(
            h(&src),
            r#"<p><a href="safe.html" ping="">y</a></p>"#,
            "{scheme}: in a non-leading token was not denied"
        );
    }
}
