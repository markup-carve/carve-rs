//! A link policy reads the host of an http(s) URL the way a browser's WHATWG
//! URL parser does, so no spelling reaches `evil.example` past a host rule.

use carve::LinkPolicy;

const BASE: Option<&str> = Some("good.example");

fn denied() -> LinkPolicy {
    LinkPolicy::unrestricted().set_denied_domains(vec!["evil.example".to_string()])
}

fn allowlist() -> LinkPolicy {
    LinkPolicy::allowlist(vec!["good.example".to_string()])
}

fn policies() -> [(&'static str, LinkPolicy); 3] {
    [
        ("denied", denied()),
        ("allowlist", allowlist()),
        ("internal_only", LinkPolicy::internal_only()),
    ]
}

const BYPASSES: &[&str] = &[
    r"https:\\evil.example/x",
    r"HTTPS:/\evil.example/x",
    "https:///evil.example/x",
    "https:evil.example/x",
    r"https://evil.example\@good.example/x",
    "https://evil.exa\tmple/x",
    "ht\ntps://evil.example/x",
    "/\t/evil.example/x",
    "https://evil%2Eexample/x",
    "https://evil.example./x",
    "https://evil\u{3002}example/x",
    "https://evil\u{FF0E}example/x",
    "https://evil\u{FF61}example/x",
];

#[test]
fn every_bypass_spelling_is_denied_under_every_host_rule() {
    for (name, policy) in policies() {
        for url in BYPASSES {
            assert!(!policy.is_url_allowed(url, BASE), "{name}: {url:?}");
        }
    }
}

#[test]
fn ordinary_good_hosts_stay_allowed() {
    for (name, policy) in policies() {
        for url in [
            "https://good.example/x",
            "https://evil.example@good.example/x",
            "https://GOOD.example./x",
            "https://good.example:8443/x",
        ] {
            assert!(policy.is_url_allowed(url, BASE), "{name}: {url:?}");
        }
    }
}

#[test]
fn a_hostless_url_passes_only_a_policy_without_host_rules() {
    assert!(LinkPolicy::unrestricted().is_url_allowed("https:", BASE));
    assert!(!denied().is_url_allowed("https:", BASE));
    assert!(!allowlist().is_url_allowed("https:///", BASE));
}

#[test]
fn configured_hosts_are_normalized_like_url_hosts() {
    let deny = |d: &str| LinkPolicy::unrestricted().set_denied_domains(vec![d.to_string()]);
    assert!(!deny("EVIL.example.").is_url_allowed("https://evil.example./x", None));
    assert!(!deny("evil.example.").is_url_allowed("https://evil.example/x", None));
    assert!(LinkPolicy::allowlist(vec!["good.example.".to_string()])
        .is_url_allowed("https://good.example/x", None));
    assert!(
        LinkPolicy::internal_only().is_url_allowed("https://good.example/x", Some("Good.Example."))
    );
}

#[test]
fn evil_hosts_in_other_positions_are_denied() {
    for (name, policy) in policies() {
        for url in [
            "https://EVIL.EXAMPLE/x",
            "https://good.example@evil.example/x",
            "https://evil.example:8443/x",
        ] {
            assert!(!policy.is_url_allowed(url, BASE), "{name}: {url:?}");
        }
    }
}
