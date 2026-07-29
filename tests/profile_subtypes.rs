//! `profiles.md` requires `autolink` and `admonition` to be nameable on their
//! own: an autolink is not a `link` (folding it in loses the authored form a
//! round trip has to restore), and an admonition is not a `div` (a profile that
//! wants to deny callouts while allowing generic containers cannot say so if
//! the kind lives in a class string).
//!
//! Both folded into the broader name before the allow/deny check, so naming
//! them was a silent no-op - a host could deny autolinks, get no error and no
//! violation, and still emit them (carve issue 362).
//!
//! They stay COVERED BY the broader name: unfolding them without that would
//! quietly widen every profile already relying on `link` or `div`.

use carve::{Options, Profile};

const AUTOLINK: &str = "See <https://example.com> here.\n";
const ADMONITION: &str = "::: note\ncallout\n:::\n";

fn html(src: &str, profile: Profile) -> String {
    carve::to_html_with_options(src, &Options::new().with_profile(profile))
}

#[test]
fn denies_an_autolink_when_the_profile_names_it() {
    assert!(!html(AUTOLINK, Profile::default().deny_inline(&["autolink"])).contains("<a "));
}

#[test]
fn still_denies_an_autolink_when_the_profile_names_link() {
    assert!(!html(AUTOLINK, Profile::default().deny_inline(&["link"])).contains("<a "));
}

#[test]
fn keeps_ordinary_links_when_only_autolink_is_denied() {
    let out = html(
        "A [real](https://a.example) and <https://b.example>.\n",
        Profile::default().deny_inline(&["autolink"]),
    );
    assert!(out.contains("href=\"https://a.example\""));
    assert!(!out.contains("href=\"https://b.example\""));
}

#[test]
fn denies_an_admonition_when_the_profile_names_it() {
    assert!(!html(ADMONITION, Profile::default().deny_block(&["admonition"])).contains("<aside"));
}

#[test]
fn still_denies_an_admonition_when_the_profile_names_div() {
    assert!(!html(ADMONITION, Profile::default().deny_block(&["div"])).contains("<aside"));
}

#[test]
fn keeps_generic_containers_when_only_admonition_is_denied() {
    // The case profiles.md names: deny callouts, allow generic containers.
    let src = format!("{ADMONITION}\n{{.wrap}}\n:::\ngeneric\n:::\n");
    let out = html(&src, Profile::default().deny_block(&["admonition"]));
    assert!(!out.contains("<aside"));
    assert!(out.contains("<div class=\"wrap\">"));
}

#[test]
fn admits_a_subtype_through_an_allow_list_naming_its_supertype() {
    let out = html(
        AUTOLINK,
        Profile::default().allow_inline(Some(&["text", "link"])),
    );
    assert!(out.contains("<a "));
}
