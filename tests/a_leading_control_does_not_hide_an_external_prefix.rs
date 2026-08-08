//! A leading URL-probe character cannot hide a protocol-relative destination
//! from a profile's external-link policy (carve-rs#839).
//!
//! Prefix classification used the raw first byte while scheme classification
//! already ignored the renderer's probe class. Thus `<DEL>//evil.com` matched
//! neither the protocol-relative nor relative branch and fell through allowed.

use carve::LinkPolicy;

const DEL: char = '\u{7f}';

#[test]
fn a_leading_del_does_not_hide_a_protocol_relative_external_url() {
    let policy = LinkPolicy::default().set_allow_external(false);
    assert!(!policy.is_url_allowed("//evil.com/x", None));
    assert!(!policy.is_url_allowed(&format!("{DEL}//evil.com/x"), None));
}

#[test]
fn relative_paths_and_fragments_remain_internal_controls() {
    let policy = LinkPolicy::default().set_allow_external(false);
    for destination in ["/local/x", "./local/x", "../local/x", "#frag"] {
        assert!(policy.is_url_allowed(destination, None), "{destination}");
    }
}

#[test]
fn prefix_classification_uses_the_normalized_view_for_internal_controls_too() {
    let allow = LinkPolicy::default().set_allow_external(false);
    let deny = LinkPolicy::default().set_allow_internal(false);
    for destination in ["/local/x", "./local/x", "../local/x", "#frag"] {
        let obscured = format!("{DEL}{destination}");
        assert!(allow.is_url_allowed(&obscured, None), "{obscured:?}");
        assert!(!deny.is_url_allowed(&obscured, None), "{obscured:?}");
    }
}

#[test]
fn a_domain_allowlist_sees_the_same_normalized_host() {
    let policy = LinkPolicy::default().set_allowed_domains(Some(vec!["good.example".to_string()]));
    assert!(policy.is_url_allowed(&format!("{DEL}//good.example/x"), None));
    assert!(!policy.is_url_allowed(&format!("{DEL}//evil.example/x"), None));
}

#[test]
fn normalization_is_for_judgement_not_a_blanket_refusal() {
    let policy = LinkPolicy::default().set_allow_external(false);
    assert!(policy.is_url_allowed("notascheme", None));
    assert!(policy.is_url_allowed(&format!("{DEL}notascheme"), None));
}
