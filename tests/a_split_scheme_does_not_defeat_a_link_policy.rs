//! A profile's link policy reads the scheme through the characters a URL
//! consumer discards (markup-carve/carve-rs#835).
//!
//! `LinkPolicy::is_url_allowed` read the text before the first colon with no
//! character filter. `trim` reaches the two ends and nothing else, so any
//! control or whitespace character INSIDE the scheme walked past the
//! denied-scheme lookup: `java<U+0001>script:alert(1)`, `java<DEL>script:` and
//! `java<U+009B>script:` were all answered allowed against the default policy,
//! while the plain `javascript:alert(1)` was answered denied.
//!
//! U+0001 is what separates this from the renderer defect carve-rs#833 fixed.
//! That character is inside PART 9 §29's class, so every renderer probe
//! stripped it even before that fix; this rule stripped nothing, so its gap was
//! the wider one.
//!
//! THIS IS A NARROWING IN ONE DIRECTION ONLY. Filtering removes characters, so
//! the deny lists can recognize more destinations and can never recognize
//! fewer; nothing a policy refuses today starts being allowed. Nothing
//! legitimate starts being refused either, because no legitimate scheme carries
//! a filtered character - a scheme is a letter followed by letters, digits,
//! `+`, `-` and `.`.
//!
//! THE ALLOWLIST FORM WAS NEVER DEFEATED and is pinned here rather than argued.
//! An earlier revision of the ticket claimed it failed open, having measured a
//! policy whose allowed list was never applied, and retracted it. An allowlist
//! refuses a scheme it does not recognize, and a split scheme is one.
//!
//! SCOPE: a document rendered with default options was never at risk, because
//! §25 blanks these destinations in the renderer whatever a profile answered.
//! What was affected is a caller using a profile to VALIDATE or FILTER, where
//! the permissive answer is the whole output.
//!
//! EVERY ASSERTION IS ON CODE POINTS, never on how a destination renders.
//! `java<DEL>script:` prints as `javascript:` in any terminal, which is how the
//! first report of the sibling defect concluded the byte had been normalized
//! away when it had not.

use carve::{LinkPolicy, Options, Profile};

/// U+0001. Inside PART 9 §29's class, so every renderer probe already saw it.
const SOH: char = '\u{1}';
/// U+007F. Outside §29 by T5.
const DEL: char = '\u{7f}';
/// U+009B. Outside §29 by T5.
const CSI: char = '\u{9b}';
/// U+00A0. Whitespace, not a control, and `trim` does not reach it here.
const NBSP: char = '\u{a0}';

fn split(mid: char) -> String {
    format!("java{mid}script:alert(1)")
}

fn points(s: &str) -> Vec<String> {
    s.chars().map(|c| format!("U+{:04X}", c as u32)).collect()
}

#[test]
fn the_split_spelling_really_carries_the_splitting_code_point() {
    // Guards every row below. If some layer normalized the character away, the
    // rows would pass for a reason that has nothing to do with the policy.
    for c in [SOH, DEL, CSI, NBSP] {
        let s = split(c);
        assert!(
            points(&s).contains(&format!("U+{:04X}", c as u32)),
            "the split spelling lost its character: {:?}",
            points(&s)
        );
        assert_ne!(s, "javascript:alert(1)");
    }
}

#[test]
fn control_the_plain_denied_spelling_is_still_refused() {
    // A CONTROL. The denylist itself was never broken; a red here means the
    // filter broke the lookup, not that the split form leaked.
    let p = LinkPolicy::default();
    assert_eq!(
        p.denied_schemes(),
        ["javascript", "vbscript", "data", "file"],
        "measured against a policy that is not the one described"
    );
    assert!(!p.is_url_allowed("javascript:alert(1)", None));
    assert!(!p.is_url_allowed(" javascript:alert(1)", None));
}

#[test]
fn control_a_legitimate_destination_is_still_allowed() {
    // A CONTROL. The filter must not start refusing anything real.
    let p = LinkPolicy::default();
    for url in [
        "https://example.com/a",
        "http://example.com/a",
        "mailto:a@example.com",
        "tel:+1234",
        "./a/b",
        "/a/b",
        "#sec",
        "//example.com/a",
        "notascheme",
    ] {
        assert!(p.is_url_allowed(url, None), "refused a legitimate {url}");
    }
}

fn refuses_split_by(c: char) {
    let url = split(c);
    assert!(
        !LinkPolicy::default().is_url_allowed(&url, None),
        "allowed a split denied scheme: {:?}",
        points(&url)
    );
}

// One test per character rather than one loop, because the four are not
// interchangeable and a narrowing of the class has to be visible as WHICH rows
// it breaks. U+0001 is inside PART 9 §29's class; the other three are not.

#[test]
fn the_denylist_refuses_a_scheme_split_by_u_0001() {
    refuses_split_by(SOH);
}

#[test]
fn the_denylist_refuses_a_scheme_split_by_u_007f() {
    refuses_split_by(DEL);
}

#[test]
fn the_denylist_refuses_a_scheme_split_by_u_009b() {
    refuses_split_by(CSI);
}

#[test]
fn the_denylist_refuses_a_scheme_split_by_u_00a0() {
    refuses_split_by(NBSP);
}

#[test]
fn every_default_denied_scheme_is_refused_in_its_split_spelling() {
    let p = LinkPolicy::default();
    for url in [
        format!("da{DEL}ta:text/html,x"),
        format!("fi{DEL}le:///etc/passwd"),
        format!("vb{DEL}script:x"),
    ] {
        assert!(!p.is_url_allowed(&url, None), "allowed {:?}", points(&url));
    }
}

#[test]
fn a_split_scheme_no_longer_skips_the_domain_denylist() {
    // The scheme read GATES the host checks: `htt<DEL>ps` was neither `http`
    // nor `https`, so a denied domain was never consulted. This engine's
    // `parse_host` splits on `://` and never looks at the scheme, so filtering
    // the scheme is the whole fix here; carve-js also had to repair the scheme
    // before its own host parse, whose pattern rejects the split spelling.
    let p = LinkPolicy::default().set_denied_domains(vec!["evil.com".to_string()]);
    assert!(!p.is_url_allowed("https://evil.com/a", None));
    for c in [SOH, DEL] {
        let url = format!("htt{c}ps://evil.com/a");
        assert!(!p.is_url_allowed(&url, None), "allowed {:?}", points(&url));
    }
    // CONTROL: an undenied host is still allowed.
    assert!(p.is_url_allowed("https://good.com/a", None));
}

#[test]
fn a_split_scheme_no_longer_skips_the_allow_external_check() {
    let p = LinkPolicy::default().set_allow_external(false);
    assert!(!p.is_url_allowed("https://example.com/a", None));
    for c in [SOH, DEL] {
        let url = format!("htt{c}ps://example.com/a");
        assert!(!p.is_url_allowed(&url, None), "allowed {:?}", points(&url));
    }
}

#[test]
fn control_the_allowlist_form_still_fails_closed_split_or_not() {
    // A CONTROL, and the claim an earlier revision of the ticket got wrong and
    // then retracted. It is pinned here so it does not have to be argued again.
    //
    // The ALLOW lookup deliberately still reads the RAW text: it asks whether a
    // scheme is exactly one it permits, and a split scheme is not. Reading the
    // probe there would START permitting `htt<DEL>ps:`, turning the fix into a
    // widening - which is what this test would catch.
    let p = LinkPolicy::default().set_allowed_schemes(Some(vec!["https".to_string()]));
    assert_eq!(
        p.allowed_schemes(),
        Some(["https".to_string()].as_slice()),
        "measured against a policy whose allowed list was never applied"
    );
    assert!(!p.is_url_allowed("javascript:alert(1)", None));
    for c in [SOH, DEL, CSI, NBSP] {
        assert!(!p.is_url_allowed(&split(c), None));
        let url = format!("htt{c}ps://example.com/a");
        assert!(
            !p.is_url_allowed(&url, None),
            "the allowlist started recognizing a split scheme: {:?}",
            points(&url)
        );
    }
    // CONTROL: the one scheme it names is still allowed.
    assert!(p.is_url_allowed("https://example.com/a", None));
}

// ---------------------------------------------------------------------------
// The three call sites that consume this answer, asserted rather than assumed.
// ---------------------------------------------------------------------------

/// Denying `https` isolates the profile from PART 9 §25: the renderer has no
/// quarrel with that scheme, so anything blanked below came from the policy.
fn https_denying_profile() -> Profile {
    Profile::full().set_link_policy(Some(
        LinkPolicy::default().set_denied_schemes(vec!["https".to_string()]),
    ))
}

fn html(src: &str, profile: Profile) -> String {
    carve::to_html_with_options(src, &Options::new().with_profile(profile))
}

#[test]
fn the_link_path_refuses_a_split_scheme() {
    let out = html(&format!("[x](htt{DEL}ps://x.com)"), https_denying_profile());
    assert!(!out.contains("href"), "the link survived: {out:?}");
    assert!(
        !out.chars().any(|c| c == DEL),
        "the destination reached the output: {:?}",
        points(&out)
    );
    // CONTROL: a scheme this policy does not deny still renders.
    let ok = html("[x](http://x.com)", https_denying_profile());
    assert!(ok.contains("href=\"http://x.com\""), "{ok:?}");
}

#[test]
fn the_inline_image_path_refuses_a_split_scheme() {
    let out = html(
        &format!("text ![alt](htt{DEL}ps://x.com) more"),
        https_denying_profile(),
    );
    assert!(!out.contains("<img"), "the image survived: {out:?}");
    assert!(
        !out.chars().any(|c| c == DEL),
        "the destination reached the output: {:?}",
        points(&out)
    );
    let ok = html("text ![alt](http://x.com) more", https_denying_profile());
    assert!(ok.contains("<img"), "{ok:?}");
}

#[test]
fn the_block_image_path_refuses_a_split_scheme() {
    // A sole image on its own line is a block node with its own gate, so this
    // is a third consumer of the same answer and not a restatement of the
    // second one.
    let out = html(
        &format!("![alt](htt{DEL}ps://x.com)"),
        https_denying_profile(),
    );
    assert!(!out.contains("<img"), "the image survived: {out:?}");
    assert!(
        !out.chars().any(|c| c == DEL),
        "the destination reached the output: {:?}",
        points(&out)
    );
    let ok = html("![alt](http://x.com)", https_denying_profile());
    assert!(ok.contains("<img"), "{ok:?}");
}
