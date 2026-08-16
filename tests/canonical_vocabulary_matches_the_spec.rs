//! The canonical vocabulary is what `profiles.md` says it is.
//!
//! `CANONICAL_BLOCK_TYPES` and `CANONICAL_INLINE_TYPES` are the strings a profile
//! can name, and spec `docs/profiles.md` calls that list normative. This engine
//! matches the page today - and nothing checked that it does.
//!
//! carve-php pins its own lists against the page and carve-js did not, which is
//! how carve-js drifted six entries: `frontmatter` on the block axis plus
//! `caption_number`, `citation_group`, `critic_comment`, `heading_ref` and
//! `substitution` on the inline one (carve-js#712). Rendering stayed correct
//! there the whole time, because the filter resolves a type on the node's own
//! axis; what lied was `is_type_allowed`, which has no axis to resolve on and so
//! falls to "allowed" for a name the vocabulary does not know. A host that asked
//! before rendering was told yes about something the renderer would deny.
//!
//! So this test exists to keep a passing state passing, and the behaviour
//! assertion below is the half that would catch the same drift here without
//! anyone thinking about vocabularies at all.

use carve::profile::{Profile, CANONICAL_BLOCK_TYPES, CANONICAL_INLINE_TYPES};

/// Block types this engine names that the PINNED page does not list yet,
/// because the clause landed after the pin and the pins move once, at the end
/// of a release.
///
/// Self-clearing: an entry the pinned page HAS listed fails below, so this
/// cannot quietly become a permanent exemption. `citation_definition` is ruled
/// a node by PART 12 section 18 (markup-carve/carve#1279), which lands in
/// `docs/profiles.md`'s Block vocabulary; carve-rs emits it from this release
/// on (markup-carve/carve#1276).
const AHEAD_OF_THE_PIN: &[&str] = &["citation_definition"];

/// The page's own list for one axis, as data.
fn spec_vocabulary(axis: &str) -> Vec<String> {
    let page = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/spec/docs/profiles.md"),
    )
    .expect("tests/spec/docs/profiles.md is missing - run `git submodule update --init`");

    let marker = format!("**{axis}:**");
    let start = page
        .find(&marker)
        .unwrap_or_else(|| panic!("no {marker} list in profiles.md"))
        + marker.len();
    let rest = &page[start..];
    let end = rest.find("\n\n").unwrap_or(rest.len());

    let mut names: Vec<String> = Vec::new();
    let mut chars = rest[..end].chars().peekable();
    while let Some(c) = chars.next() {
        if c != '`' {
            continue;
        }
        let mut name = String::new();
        while let Some(&next) = chars.peek() {
            chars.next();
            if next == '`' {
                break;
            }
            name.push(next);
        }
        if !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c == '_' || c.is_ascii_digit())
        {
            names.push(name);
        }
    }
    names.sort();
    names.dedup();
    names
}

/// The page's list for one axis, plus the names this engine is ahead of the pin
/// on. The Inline axis has none, so it is the page's list unchanged.
fn expected_vocabulary(axis: &str) -> Vec<String> {
    let mut names = spec_vocabulary(axis);
    if axis == "Block" {
        for pending in AHEAD_OF_THE_PIN {
            assert!(
                !names.iter().any(|n| n == pending),
                "the pinned profiles.md now lists `{pending}` - drop it from AHEAD_OF_THE_PIN"
            );
            names.push((*pending).to_string());
        }
        names.sort();
    }
    names
}

fn sorted(types: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = types.iter().map(|t| (*t).to_string()).collect();
    out.sort();
    out
}

#[test]
fn the_page_was_actually_read() {
    // Without this a missing submodule or a renamed heading would make both
    // comparisons below vacuous rather than failing.
    assert!(
        spec_vocabulary("Block").len() > 20,
        "block vocabulary looks empty: {:?}",
        spec_vocabulary("Block")
    );
    assert!(
        spec_vocabulary("Inline").len() > 20,
        "inline vocabulary looks empty: {:?}",
        spec_vocabulary("Inline")
    );
}

#[test]
fn the_block_vocabulary_matches_the_spec() {
    // BOTH directions: a type the page lists and this engine cannot name is a
    // deny a host cannot express, and a type this engine names that the page
    // omits is a name the spec never promised.
    assert_eq!(sorted(CANONICAL_BLOCK_TYPES), expected_vocabulary("Block"));
}

#[test]
fn the_inline_vocabulary_matches_the_spec() {
    assert_eq!(
        sorted(CANONICAL_INLINE_TYPES),
        expected_vocabulary("Inline")
    );
}

#[test]
fn denying_a_spec_listed_type_reaches_the_string_api() {
    // The consequence, as behaviour rather than as list membership. This is what
    // went wrong in carve-js: the filter denied the type and `is_type_allowed`
    // said it was allowed, because the name was outside the vocabulary.
    for axis in ["Block", "Inline"] {
        for name in expected_vocabulary(axis) {
            let profile = if axis == "Block" {
                Profile::full().deny_block(&[name.as_str()])
            } else {
                Profile::full().deny_inline(&[name.as_str()])
            };
            assert!(
                !profile.is_type_allowed(&name),
                "{axis} type `{name}` is denied, but is_type_allowed says it is allowed - \
                 the name is outside CANONICAL_{}_TYPES",
                axis.to_uppercase()
            );
        }
    }
}
