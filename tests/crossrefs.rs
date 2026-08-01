use carve::{parse_with_options, to_json, Options};

#[test]
fn standalone_crossref_serializes_as_heading_ref_with_source_span() {
    let doc = parse_with_options(
        "# Some Title\n\nSee </#some-title> here.\n",
        &Options::new().with_positions(true),
    );
    let json = to_json(&doc);
    assert!(json.contains(r#""type":"heading_ref","target":"some-title""#));
    assert!(json.contains(
        r#""pos":{"startLine":3,"endLine":3,"startColumn":5,"endColumn":19,"startOffset":18,"endOffset":32}"#
    ));
    assert!(!json.contains(r#""fromCrossref""#), "{json}");
}

#[test]
fn crossref_in_link_label_still_serializes_as_heading_ref() {
    let json = carve::to_json_with_options(
        "# Some Title\n\n[see </#some-title>](/outer)",
        &Options::new(),
    );
    assert!(json.contains(r#""type":"heading_ref","target":"some-title""#));
    assert!(!json.contains(r#""fromCrossref""#), "{json}");
}

#[test]
fn crossref_in_link_label_renders_text_without_nested_anchor() {
    let html = carve::to_html("# Some Title\n\n[see </#some-title>](/outer)");
    assert!(
        html.contains(r#"<a href="/outer">see Some Title</a>"#),
        "{html}"
    );
    assert!(!html.contains(r##"href="#some-title""##), "{html}");
}

#[test]
fn fmt_round_trips_crossref_source() {
    let src = "# Some Title\n\nSee </#some-title>.\n";
    let out = carve::to_carve(src);
    assert_eq!(out, src);
}

/// Every target resolves the reference itself, and each one keeps the form it
/// already used for a link to the same heading. Emitting only the title would
/// silently drop the link from the Markdown and ANSI exports - the mistake this
/// change was originally written with, caught by the golden fixtures.
///
/// An UNRESOLVED reference degrades to its literal source in every target,
/// which is the one case where the bare `</#…>` text is correct.
#[test]
fn non_html_renderers_resolve_crossrefs_at_render_time() {
    let src = "# Some Title\n\nSee </#some-title> and </#missing>.";

    // Plain text has no link form, so the title alone IS the link.
    assert_eq!(
        carve::to_plain_text(src),
        "Some Title\n\nSee Some Title and </#missing>.\n"
    );

    // Markdown has one, and uses it.
    assert_eq!(
        carve::to_markdown(src),
        "# Some Title {#Some-Title}\n\nSee [Some Title](#Some-Title) and </#missing>.\n"
    );

    // ANSI styles it underlined-blue exactly as it styles a link, and suppresses
    // the `(href)` suffix because the destination is a fragment.
    let ansi = carve::to_ansi(src);
    assert!(
        ansi.contains("\x1b[4m\x1b[34mSome Title\x1b[0m"),
        "a resolved reference lost its link styling: {ansi:?}"
    );
    assert!(ansi.contains("</#missing>"), "{ansi:?}");
}
