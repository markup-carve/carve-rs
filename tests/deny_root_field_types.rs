//! `profiles.md` lists `frontmatter` and `footnote` in the normative Block
//! vocabulary, so a profile can name them. This engine keeps both on the
//! `Document` rather than in `children`, and the filter walked only
//! `children` - so naming either did nothing at all: no violation, no change
//! (carve#422).
//!
//! Rendered output is unchanged either way, because neither type renders. That
//! is exactly why these tests do not stop at comparing output.

use carve::profile::Profile;
use carve::profile_filter::apply_profile;

const WITH_FRONTMATTER: &str = "---\ntitle: Secret\napi_key: sk-123\n---\n\nBody.\n";
const WITH_FOOTNOTE: &str = "Body[^a].\n\n[^a]: note\n";

#[test]
fn denying_frontmatter_removes_it_and_reports() {
    let doc = carve::parse(WITH_FRONTMATTER);
    assert!(
        doc.frontmatter_raw.is_some(),
        "the fixture lost its frontmatter"
    );

    let profile = Profile::full().deny_block(&["frontmatter"]);
    let result = apply_profile(doc, &profile, None).expect("to_text action must not raise");

    assert!(
        result.doc.frontmatter_raw.is_none(),
        "frontmatter survived the deny"
    );
    assert!(
        result.doc.frontmatter.is_empty(),
        "the parsed map survived the deny"
    );
    let types: Vec<&str> = result
        .violations
        .iter()
        .map(|v| v.node_type.as_str())
        .collect();
    assert_eq!(types, vec!["frontmatter"]);
}

#[test]
fn denying_footnote_definitions_removes_them_and_reports() {
    let doc = carve::parse(WITH_FOOTNOTE);
    assert!(
        !doc.footnote_defs.is_empty(),
        "the fixture lost its definition"
    );

    let profile = Profile::full().deny_block(&["footnote"]);
    let result = apply_profile(doc, &profile, None).expect("to_text action must not raise");

    assert!(
        result.doc.footnote_defs.is_empty(),
        "definitions survived the deny"
    );
    let types: Vec<&str> = result
        .violations
        .iter()
        .map(|v| v.node_type.as_str())
        .collect();
    assert_eq!(types, vec!["footnote"]);
}

#[test]
fn a_profile_that_denies_nothing_keeps_both() {
    let doc = carve::parse(WITH_FRONTMATTER);
    let result = apply_profile(doc, &Profile::full(), None).expect("full profile must not raise");

    assert!(result.doc.frontmatter_raw.is_some());
    assert!(result.violations.is_empty());
}

#[test]
fn rendered_output_is_unchanged_either_way() {
    // Deliberate, and the reason the assertions above look at the tree and the
    // violations instead: neither type renders, so a caller who denies one of
    // these and diffs the output sees nothing change.
    let plain = carve::to_html(WITH_FRONTMATTER);

    let doc = carve::parse(WITH_FRONTMATTER);
    let profile = Profile::full().deny_block(&["frontmatter"]);
    let filtered = apply_profile(doc, &profile, None).expect("to_text action must not raise");

    assert_eq!(
        carve::render_html(&filtered.doc)
            .expect("the tree under test is within the render ceiling"),
        plain
    );
}
