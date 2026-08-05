//! Denying a definition that renders nothing changes nothing on the page.
//!
//! The filter degrades a denied node to its text content, and a node it cannot
//! extract text from takes a deliberate diagnostic path: record a
//! `to_text_yielded_nothing` violation and substitute `[<canonical>]`, a marker
//! chosen to be ugly enough that it cannot pass for intended output. That path
//! was doing its job - these nodes should never have reached it (carve-rs#645).
//!
//! Both definition kinds render NOTHING where they were written; the `link`,
//! `image` or `abbreviation` they feed is what appears on the page. So denying
//! one has nothing to substitute, and degrading it publishes something the
//! author never wrote at that position:
//!
//! - a link reference definition extracted to nothing, so it printed the
//!   engine's own type name, `<p>[link_reference_definition]</p>`;
//! - an abbreviation definition extracted to its EXPANSION, so it published
//!   `<p>HyperText</p>` - worse in a way, because it looks like content.
//!
//! docs/profiles.md names the two together and requires the rendered HTML to be
//! byte-identical either way, as it already is for `comment` and `frontmatter`.
//! carve-js reached the same place in carve-js#702, having added the
//! `abbreviation_def` arm earlier and left this one behind.
//!
//! WHAT MUST NOT CHANGE: the link still resolves and the `<abbr>` still renders.
//! Denying a definition denies the definition - the node that stops an `<abbr>`
//! reaching the page is `abbreviation`. And the violation is still recorded, so
//! removing the node does not trade a visible wrong output for a silent one.

use carve::profile::Profile;

const LINK_REF: &str = "See [x][y].\n\n[y]: /u\n";
const ABBR: &str = "HTML is fine.\n\n*[HTML]: HyperText\n";

fn html(src: &str, deny: &[&str]) -> String {
    let options = carve::Options {
        profile: Some(Profile::full().deny_block(deny)),
        ..Default::default()
    };
    carve::to_html_with_options(src, &options)
}

/// The violations a denial records, as (type, reason) pairs.
fn violations(src: &str, deny: &[&str]) -> Vec<(String, String)> {
    let profile = Profile::full().deny_block(deny);
    let doc = carve::parse(src);
    carve::profile_filter::apply_profile(doc, &profile, None)
        .expect("collect mode does not error")
        .violations
        .into_iter()
        .map(|v| (v.node_type, v.reason))
        .collect()
}

#[test]
fn a_denied_link_reference_definition_leaves_no_marker() {
    let out = html(LINK_REF, &["link_reference_definition"]);
    assert!(
        !out.contains("[link_reference_definition]"),
        "the engine's own type name reached the page: {out}"
    );
    assert_eq!(out.trim(), "<p>See <a href=\"/u\">x</a>.</p>", "{out}");
}

#[test]
fn a_denied_abbreviation_definition_does_not_publish_its_expansion() {
    let out = html(ABBR, &["abbreviation_def"]);
    // `HyperText` still appears - inside the `title`, where the author put it.
    // The defect was a PARAGRAPH of it, so assert on the shape, not the word.
    assert!(!out.contains("<p>HyperText</p>"), "{out}");
    assert_eq!(
        out.trim(),
        "<p><abbr title=\"HyperText\">HTML</abbr> is fine.</p>",
        "{out}"
    );
}

#[test]
fn the_rendered_html_is_byte_identical_either_way() {
    // The rule profiles.md states for this category, asserted as a comparison
    // rather than against a pinned string - which is what makes it about the
    // category and not about these two documents.
    for (src, deny) in [
        (LINK_REF, "link_reference_definition"),
        (ABBR, "abbreviation_def"),
    ] {
        assert_eq!(
            html(src, &[deny]),
            html(src, &[]),
            "denying {deny} changed the output"
        );
    }
}

#[test]
fn the_denial_is_still_reported() {
    // Removing the node quietly would trade one silent failure for another: a
    // host in error mode has to still learn the document carried a definition.
    let found = violations(LINK_REF, &["link_reference_definition"]);
    assert!(
        found
            .iter()
            .any(|(node_type, _)| node_type == "link_reference_definition"),
        "{found:?}"
    );

    let found = violations(ABBR, &["abbreviation_def"]);
    assert!(
        found
            .iter()
            .any(|(node_type, _)| node_type == "abbreviation_def"),
        "{found:?}"
    );
}

#[test]
fn the_reason_is_no_longer_a_missing_extractor() {
    // The old violation said `to_text_yielded_nothing` - the engine reporting a
    // gap in itself. Now the reason is the denial, which is what happened.
    let found = violations(LINK_REF, &["link_reference_definition"]);
    assert!(
        !found
            .iter()
            .any(|(_, reason)| reason == "to_text_yielded_nothing"),
        "still reporting a missing extractor: {found:?}"
    );
}

#[test]
fn a_comment_still_goes_the_same_way() {
    // The arm this joins, pinned so a change to the predicate cannot trade it.
    assert_eq!(html("a\n\n%% hidden\n", &["comment"]).trim(), "<p>a</p>");
}

#[test]
fn denying_the_node_that_does_render_still_works() {
    // The boundary that keeps "definitions are removed" from being read as
    // "an abbreviation cannot be denied at all": denying `abbreviation` is how a
    // host stops the `<abbr>`, and it still does. It is an INLINE type - the
    // definition is the block one, and mixing the two up is the whole confusion
    // this pair of names invites.
    let options = carve::Options {
        profile: Some(Profile::full().deny_inline(&["abbreviation"])),
        ..Default::default()
    };
    let out = carve::to_html_with_options(ABBR, &options);
    assert!(!out.contains("<abbr"), "{out}");
    assert!(out.contains("HTML is fine."), "{out}");
}
