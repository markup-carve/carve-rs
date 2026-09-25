//! A container whose only content is a title or a `[label]` survives a profile
//! (carve-rs#1897).
//!
//! `CARVE-P10-001` counts a rendered title or div label as visible container
//! content, so the body slot is filled and nothing is missing from the
//! document. The profile filter's empty-container cleanup answered on
//! `children` alone, so every profile deleted the whole container and the
//! authored title went with it - on a document the profile had no quarrel with.
//!
//! Every case is asserted against the SAME input rendered without a profile, so
//! a build whose renderer had simply stopped emitting titles cannot pass.

use carve::{
    to_ansi_with_options, to_carve, to_carve_with_options, to_html, to_html_with_options,
    to_markdown_with_options, to_plain_text_with_options, DisallowedAction, Options, Profile,
};

/// Profiles that allow containers. Each one must leave these documents alone.
fn permissive_profiles() -> Vec<(&'static str, Profile)> {
    vec![
        ("full", Profile::full()),
        ("article", Profile::article()),
        // The reproducer from the ticket: a denial the document never uses.
        (
            "deny_inline(link)",
            Profile::default().deny_inline(&["link"]),
        ),
        (
            "deny_block(raw_block)",
            Profile::default().deny_block(&["raw_block"]),
        ),
    ]
}

fn html(source: &str, profile: Profile) -> String {
    to_html_with_options(source, &Options::new().with_profile(profile))
}

fn carve(source: &str, profile: Profile) -> String {
    to_carve_with_options(source, &Options::new().with_profile(profile))
}

/// The four shapes markup-carve/carve#2272 added to the corpus, which pin what
/// fills a container's body slot. Rendered here WITHOUT a profile; the profiled
/// render has to match byte for byte.
const BODY_SLOT_ROWS: &[&str] = &[
    // a title only
    "::: note \"Careful\"\n:::\n",
    // a label only, on a bare div and on a named one
    "::: [First]\n:::\n\n::: note [End]\n:::\n",
    // a title with a body
    "::: note \"Careful\"\nVisible body.\n:::\n",
    // a title and a label together
    "::: note \"Careful\" [End]\n:::\n",
];

/// A directive marker carries the same two authored fields, and losing it costs
/// the placement as well as the string.
const DIRECTIVE_ROWS: &[&str] = &[
    "::: toc \"Contents\"\n:::\n\n# H\n",
    "::: toc [T]\n:::\n\n# H\n",
    "::: footnotes \"Notes\"\n:::\n",
];

/// A bare placement marker: no title, no label, no body (carve-rs#1922).
///
/// Its kind IS its content - it says WHERE generated content goes, and that
/// effect is elsewhere in the document, so an emptiness test reading `children`
/// cannot see it. A profile denies the constructs it names, and none of these
/// names a denied inline or a denied block, so deleting one is collateral: what
/// goes with it is the only thing in the document saying where the table of
/// contents, the bibliography or the endnotes belong.
///
/// All six kinds of `GENERATED_CONTENT_KINDS`, because the seam reads the kind
/// list rather than one word.
const PLACEMENT_ROWS: &[&str] = &[
    "::: toc\n:::\n\n# H\n",
    "::: footnotes\n:::\n",
    "::: bibliography\n:::\n",
    "::: glossary\n:::\n",
    "::: index\n:::\n",
    "::: references\n:::\n",
];

#[test]
fn the_unprofiled_render_is_what_the_corpus_pins() {
    // The oracle for every assertion below. Taken from the clause via
    // carve#2275's sidecars, not from this build's writer.
    assert_eq!(
        to_html(BODY_SLOT_ROWS[0]),
        "<aside class=\"admonition note\" aria-labelledby=\"adm-1\">\n  <p class=\"admonition-title\" id=\"adm-1\">Careful</p>\n</aside>"
    );
    assert_eq!(
        to_html(BODY_SLOT_ROWS[1]),
        "<div>\n  <p class=\"div-label\">First</p>\n</div>\n<aside class=\"admonition note\" aria-label=\"Note\">\n  <p class=\"div-label\">End</p>\n</aside>"
    );
    assert_eq!(
        to_html(BODY_SLOT_ROWS[2]),
        "<aside class=\"admonition note\" aria-labelledby=\"adm-1\">\n  <p class=\"admonition-title\" id=\"adm-1\">Careful</p>\n  <p>Visible body.</p>\n</aside>"
    );
}

#[test]
fn every_permissive_profile_keeps_a_title_or_label_only_container() {
    for source in BODY_SLOT_ROWS {
        let unprofiled = to_html(source);
        assert!(!unprofiled.is_empty(), "{source:?}");
        for (name, profile) in permissive_profiles() {
            assert_eq!(html(source, profile), unprofiled, "{name} / {source:?}");
        }
    }
}

#[test]
fn every_permissive_profile_keeps_a_directive_marker_with_a_title_or_label() {
    for source in DIRECTIVE_ROWS {
        let unprofiled = to_html(source);
        for (name, profile) in permissive_profiles() {
            assert_eq!(html(source, profile), unprofiled, "{name} / {source:?}");
        }
    }
}

#[test]
fn every_permissive_profile_keeps_a_bare_placement_marker() {
    // The unprofiled bytes are pinned first, so a build whose renderer had
    // stopped emitting the floor cannot pass by matching nothing to nothing.
    assert_eq!(
        to_html(PLACEMENT_ROWS[1]),
        "<div class=\"footnotes\">\n\n</div>"
    );
    for source in PLACEMENT_ROWS {
        let unprofiled = to_html(source);
        assert!(unprofiled.contains("class=\""), "{source:?}");
        for (name, profile) in permissive_profiles() {
            assert_eq!(html(source, profile), unprofiled, "{name} / {source:?}");
        }
    }
}

#[test]
fn the_carve_writer_keeps_the_block_under_every_permissive_profile() {
    for source in BODY_SLOT_ROWS
        .iter()
        .chain(DIRECTIVE_ROWS)
        .chain(PLACEMENT_ROWS)
    {
        let unprofiled = to_carve(source);
        for (name, profile) in permissive_profiles() {
            assert_eq!(carve(source, profile), unprofiled, "{name} / {source:?}");
        }
    }
}

// ---- controls ----

#[test]
fn a_container_with_a_body_is_unaffected_by_every_profile() {
    // The control that separates "the cleanup is too eager" from "the profile
    // is removing containers": a body-bearing container never went missing.
    for source in [
        "::: note\nVisible body.\n:::\n",
        ":::\nVisible body.\n:::\n",
        "::: toc\nVisible body.\n:::\n",
    ] {
        let unprofiled = to_html(source);
        assert!(unprofiled.contains("Visible body."), "{source:?}");
        for (name, profile) in permissive_profiles() {
            assert_eq!(html(source, profile), unprofiled, "{name} / {source:?}");
        }
    }
}

#[test]
fn a_container_with_neither_title_label_nor_body_still_collapses() {
    // Unchanged behavior, and the reason the predicate reads the two authored
    // fields rather than being switched off: with nothing authored inside, a
    // profiled document loses no text by dropping the shell. carve#2275's
    // fourth row is this control, and its unprofiled bytes are pinned here so
    // the blank body line cannot quietly go missing instead.
    //
    // A bare `::: toc` used to be in this list and is now in `PLACEMENT_ROWS`:
    // it carries a placement, which is content the profile never denied
    // (carve-rs#1922). An anonymous div and a bodyless admonition carry
    // nothing, and still go.
    assert_eq!(
        to_html("::: note\n:::\n\n:::\n:::\n"),
        "<aside class=\"admonition note\" aria-label=\"Note\">\n\n</aside>\n<div>\n\n</div>"
    );
    for source in ["::: note\n:::\n", ":::\n:::\n"] {
        assert!(!to_html(source).is_empty(), "{source:?}");
        for (name, profile) in permissive_profiles() {
            assert_eq!(html(source, profile), "", "{name} / {source:?}");
        }
    }
}

#[test]
fn the_text_targets_keep_the_title_under_every_permissive_profile() {
    // The loss is not HTML-shaped: the Markdown, plain-text and ANSI writers
    // all emit the title, so dropping the node took the string out of each.
    let source = "::: note \"Careful\"\n:::\n";
    for (name, profile) in permissive_profiles() {
        let options = Options::new().with_profile(profile);
        assert!(
            to_markdown_with_options(source, &options).contains("Careful"),
            "{name} markdown"
        );
        assert!(
            to_plain_text_with_options(source, &options).contains("Careful"),
            "{name} plain"
        );
        assert!(
            to_ansi_with_options(source, &options).contains("Careful"),
            "{name} ansi"
        );
    }
}

#[test]
fn a_title_the_profile_emptied_collapses_with_the_container() {
    // The counter-case. Under `Strip` nothing authored survives the denial, so
    // the shell goes too - the only reason the predicate cannot read
    // `title.is_some()`. The default `ToText` keeps the link's text, and with
    // text left the container stays, which is the pair that shows the predicate
    // reads content rather than presence.
    let source = "::: note \"[gone](https://example.com)\"\n:::\n";
    assert!(to_html(source).contains("gone"), "{}", to_html(source));

    let stripped = Profile::default()
        .deny_inline(&["link"])
        .on_disallowed(DisallowedAction::Strip);
    assert_eq!(html(source, stripped), "");

    let to_text = Profile::default().deny_inline(&["link"]);
    assert_eq!(
        html(source, to_text),
        "<aside class=\"admonition note\" aria-labelledby=\"adm-1\">\n  <p class=\"admonition-title\" id=\"adm-1\">gone</p>\n</aside>"
    );
}

#[test]
fn a_profile_that_denies_the_container_still_removes_it() {
    // `comment` and `minimal` allowlist blocks that exclude containers, so the
    // container is denied on its own terms and the cleanup is not what removes
    // it. Pinned so the fix cannot be mistaken for "profiles keep everything".
    for source in BODY_SLOT_ROWS {
        for (name, profile) in [
            ("comment", Profile::comment()),
            ("minimal", Profile::minimal()),
        ] {
            let out = html(source, profile);
            assert!(
                !out.contains("admonition note") && !out.contains("div-label"),
                "{name} / {source:?}: {out}"
            );
        }
    }
}

#[test]
fn a_profile_that_denies_containers_still_removes_a_placement_carrier() {
    // The other direction for carve-rs#1922, and the line the ruling draws:
    // keeping a placement alive against the EMPTINESS cleanup says nothing
    // about a profile that denies the construct itself. `comment` and `minimal`
    // allowlist blocks that exclude containers, so every placement carrier is
    // refused there and degrades to the disallowed-block placeholder.
    for source in PLACEMENT_ROWS.iter().chain(DIRECTIVE_ROWS) {
        for (name, profile) in [
            ("comment", Profile::comment()),
            ("minimal", Profile::minimal()),
        ] {
            let out = html(source, profile);
            assert!(
                !out.contains("class=\""),
                "{name} / {source:?} kept a container: {out}"
            );
            assert!(
                out.contains("[directive]"),
                "{name} / {source:?} did not report the refusal: {out}"
            );
        }
    }
}
