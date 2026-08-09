//! A profile classifies URL prefixes the way a WHATWG URL parser does
//! (follow-up to carve-rs#844; parity with carve-js#927).

use carve::LinkPolicy;

fn internal_only() -> LinkPolicy {
    LinkPolicy::default().set_allow_external(false)
}

#[test]
fn every_leading_ascii_c0_authority_is_external() {
    for codepoint in 0..=0x20 {
        let prefix = char::from_u32(codepoint).unwrap();
        let url = format!("{prefix}//evil.com/x");
        assert!(
            !internal_only().is_url_allowed(&url, None),
            "U+{codepoint:04X}"
        );
    }
}

#[test]
fn backslash_authority_spellings_are_external() {
    for url in [
        r"\//evil.com/x",
        r"\\evil.com/x",
        r"/\evil.com/x",
        r"\/evil.com/x",
    ] {
        assert!(!internal_only().is_url_allowed(url, None), "{url:?}");
    }
}

#[test]
fn url_significant_prefixes_remain_relative_content() {
    for prefix in [
        '\u{7f}', '\u{80}', '\u{9f}', '\u{a0}', '\u{1680}', '\u{feff}',
    ] {
        let url = format!("{prefix}//evil.com/x");
        assert!(internal_only().is_url_allowed(&url, None), "{url:?}");
    }
}

#[test]
fn ordinary_controls_remain_honest() {
    let policy = internal_only();
    assert!(policy.is_url_allowed("/local/x", None));
    assert!(policy.is_url_allowed("#frag", None));
    assert!(policy.is_url_allowed("page.crv", None));
    assert!(!policy.is_url_allowed("//evil.com/x", None));
    assert!(!policy.is_url_allowed("https://evil.com/x", None));
}

#[test]
fn a_domain_allowlist_reads_the_normalized_authority() {
    let policy = LinkPolicy::default().set_allowed_domains(Some(vec!["good.example".to_string()]));
    assert!(policy.is_url_allowed(r"\\good.example/x", None));
    assert!(!policy.is_url_allowed(r"\\evil.example/x", None));
}
