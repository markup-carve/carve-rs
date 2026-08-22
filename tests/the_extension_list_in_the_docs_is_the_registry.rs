//! `docs/extensions.md` names the extensions this crate ships, and says which
//! path produces a tab group's static output. Both were wrong
//! (markup-carve/carve-rs#1221): the list had four entries where the registry
//! has 32 keys, and the doc explained the flattening of a tab group as the work
//! of the core caption floor "because carve-rs has no Tabs / CodeGroup
//! extension" - which both extensions had contradicted since PR #906.
//!
//! A HAND-MAINTAINED LIST IS WHAT WENT STALE, so the list is gated here rather
//! than re-typed correctly and left to rot again. The prose around it cannot be
//! gated, so the claims it makes about which code path runs are pinned by the
//! measurements below instead.

use carve::extensions::registry;
use carve::{CarveExtension, CodeGroup, Mode, Options, Tabs};

const DOC: &str = include_str!("../docs/extensions.md");
const MARKER: &str = "<!-- registry-keys:";

const TAB_SOURCE: &str = ":::: tabs\n::: tab [Rust]\nrust body\n:::\n::::\n";
const GROUP_SOURCE: &str = ":::: code-group\n``` rust [Cargo]\nfn main() {}\n```\n::::\n";

fn html(source: &str, ext: Option<&dyn CarveExtension>, mode: Mode) -> String {
    let mut opts = Options::new().with_mode(mode);
    if let Some(ext) = ext {
        opts = opts.with_extension(ext);
    }
    carve::to_html_with_options(source, &opts)
}

/// The keys the doc's marked block lists, in the order it lists them.
fn documented_keys() -> Vec<String> {
    let after = DOC
        .split_once(MARKER)
        .expect("the registry-keys marker is gone from docs/extensions.md")
        .1;
    let fenced = after
        .split_once("```")
        .expect("no fenced block after the marker")
        .1;
    let body = fenced
        .split_once("```")
        .expect("the fenced block after the marker never closes")
        .0;
    body.split(',')
        .map(|key| key.split_whitespace().collect::<String>())
        .filter(|key| !key.is_empty())
        .collect()
}

#[test]
fn the_documented_list_is_exactly_the_registry() {
    let mut documented = documented_keys();
    let mut actual: Vec<String> = registry::keys().map(str::to_string).collect();
    documented.sort();
    actual.sort();
    assert_eq!(
        documented, actual,
        "docs/extensions.md and carve::extensions::registry disagree"
    );
}

#[test]
fn the_documented_counts_match_the_registry() {
    let keys = registry::keys().count();
    let modules: std::collections::BTreeSet<&str> =
        registry::REGISTRY.iter().map(|e| e.module).collect();
    assert!(
        DOC.contains(&format!(
            "**{} extension modules under {keys} registry keys**",
            modules.len()
        )),
        "the doc's counts are not {} modules / {keys} keys",
        modules.len()
    );
}

/// The claim the false premise was propping up: a REGISTERED Tabs or CodeGroup
/// flattens its own group, and never reaches the core caption floor.
#[test]
fn a_registered_group_extension_emits_its_own_static_output() {
    let tabs = Tabs::new();
    let out = html(TAB_SOURCE, Some(&tabs), Mode::Static);
    assert!(
        out.contains("<section class=\"tabs-panel\">")
            && out.contains("<h3 class=\"tabs-label\">Rust</h3>"),
        "the Tabs static arm did not run: {out}"
    );
    assert!(
        !out.contains("div-label"),
        "the caption floor ran for a registered Tabs: {out}"
    );

    let group = CodeGroup::new();
    let out = html(GROUP_SOURCE, Some(&group), Mode::Static);
    assert!(
        out.contains("<section class=\"code-group-panel\">")
            && out.contains("<h3 class=\"code-group-label\">Cargo</h3>"),
        "the CodeGroup static arm did not run: {out}"
    );
    assert!(
        !out.contains("div-label"),
        "the caption floor ran for a registered CodeGroup: {out}"
    );
}

/// And the other half: the floor is real, and it is what an UNREGISTERED tab
/// group degrades through. That is the case the corrected paragraph describes.
#[test]
fn the_caption_floor_is_what_runs_with_no_extension_registered() {
    let out = html(TAB_SOURCE, None, Mode::Static);
    assert!(
        out.contains("<p class=\"div-label\">Rust</p>"),
        "the caption floor did not run for an unregistered tab group: {out}"
    );
    assert!(
        !out.contains("tabs-panel"),
        "a tab panel appeared with no Tabs extension registered: {out}"
    );
}

/// The doc's Details row said `<section class="details">`. This engine has
/// never emitted that: it keeps the disclosure and forces `open`.
#[test]
fn details_keeps_its_disclosure_in_static_mode() {
    let details = carve::Details;
    let source = "::: details\nbody\n:::\n";
    let out = html(source, Some(&details), Mode::Static);
    assert!(out.contains("<details open"), "{out}");
    assert!(!out.contains("class=\"details\""), "{out}");
}

/// The set of extensions whose output DEPENDS on the mode, DERIVED rather than
/// re-typed - the whole point of this file is that a hand-maintained list rots.
///
/// A module is mode-aware when it asks the render context for the mode. That is
/// the only way an extension can branch on it: `Mode` is not otherwise reachable
/// from a render hook. So the set is every registered module whose source
/// mentions `is_static`, read from disk at test time - which means a NEW
/// mode-aware extension fails here rather than passing unnoticed, and so does
/// one that stops branching.
///
/// An earlier version of this test named the six and checked that each key
/// exists and each display name appears somewhere in the document. That could
/// not fail for either of the changes it was supposed to catch: a seventh
/// mode-aware extension still leaves all six keys registered and all six names
/// in the file. It was a check that could not detect what it claimed to.
#[test]
fn the_documented_static_aware_extensions_are_the_ones_that_branch_on_the_mode() {
    let mut derived: Vec<String> = registry::REGISTRY
        .iter()
        .map(|entry| entry.module)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .filter(|module| {
            // Most registered modules live in `src/extensions/`; `citations` is
            // a registered extension that lives beside it. A module the
            // registry names and neither path holds is a broken registry, so it
            // fails here rather than silently reading as mode-independent.
            let source = [
                format!("src/extensions/{module}.rs"),
                format!("src/{module}.rs"),
            ]
            .into_iter()
            .find_map(|path| std::fs::read_to_string(path).ok())
            .unwrap_or_else(|| panic!("no source file for registered module {module}"));
            source.contains("is_static")
        })
        .map(str::to_string)
        .collect();
    derived.sort();

    let mut expected = vec![
        "code_group".to_string(),
        "details".to_string(),
        "fenced_render".to_string(),
        "math_block".to_string(),
        "spoiler".to_string(),
        "tabs".to_string(),
    ];
    expected.sort();
    assert_eq!(
        derived, expected,
        "the set of mode-aware extension modules moved; update the \"Six of them\" \
         paragraph in docs/extensions.md to match"
    );

    // And the paragraph names the same six. Compared against the document with
    // its line wrapping normalized away, so re-wrapping the prose does not fail
    // a test about which extensions it names.
    let flowed = DOC.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        flowed.contains("**Details, Spoiler, Tabs, CodeGroup, FencedRender and MathBlock**"),
        "docs/extensions.md no longer names the six mode-aware extensions"
    );
}
