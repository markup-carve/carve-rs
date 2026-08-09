//! Two `carve`-target divergences from carve#352, both of them this engine
//! disagreeing with carve-js about what the writer should say.

/// An escaped space is written back as an ESCAPED SPACE.
///
/// Resolving it to a real no-break space lost the distinction the parser draws:
/// the source `10\ kg` came back carrying U+00A0, which re-parses as a literal
/// nbsp rather than as an escape, so the text node differed even though the HTML
/// did not. carve-js fixed that in carve#369.
#[test]
fn an_escaped_space_survives_as_an_escape() {
    assert_eq!(carve::to_carve("10\\ kg\n"), "10\\ kg\n");
    assert_eq!(
        carve::to_carve("say\\ 'twas a fine\\ \"day\"\n"),
        "say\\ 'twas a fine\\ \"day\"\n"
    );
}

/// The backslash an escaped space expands to is ITSELF an unconditional escape,
/// so the expansion has to happen after escaping. Doing it during rendering let
/// the escaper double it, giving `10\\ kg`.
#[test]
fn the_expanded_backslash_is_not_doubled() {
    let out = carve::to_carve("10\\ kg\n");
    assert!(!out.contains("\\\\"), "backslash was doubled: {out:?}");
    assert_eq!(carve::to_html(&out), carve::to_html("10\\ kg\n"));
    assert_eq!(carve::to_carve(&out), out, "fmt is not idempotent");
}

#[test]
fn an_escaped_space_at_line_end_loses_the_space_inside_a_list_item() {
    let source = "- item\n\\ \nx\n";
    let expected = "- item\n  \\\n  x\n";
    let out = carve::to_carve(source);

    assert_eq!(out, expected);
    assert_eq!(carve::to_html(&out), carve::to_html(source));
    assert_eq!(carve::to_carve(&out), out, "fmt is not idempotent");
}

#[test]
fn a_definition_between_a_table_and_caption_prevents_attachment() {
    let source = "^ cap\n# head\n{.cls}\n@user\n| a | b |\n[a]: /u\n^ cap\n";
    let html = carve::to_html(source);

    assert!(
        !html.contains("<caption>"),
        "definition was skipped: {html}"
    );
    assert!(html.contains("<p>^ cap</p>"));
    assert_eq!(
        carve::to_carve(source),
        "^ cap\n\n# head\n\n{.cls}\n@user\n\n| a | b |\n\n\\^ cap\n\n[a]: /u\n"
    );
}

/// A line block's leading indentation still resolves to ORDINARY spaces: that is
/// the source form the parser reads back as indentation, whereas an escape or a
/// real nbsp re-parses as literal text (carve#359).
#[test]
fn a_line_block_indent_is_still_plain_spaces() {
    let src = "::: |\nRoses are red,\n  indented line.\n:::\n";
    let out = carve::to_carve(src);
    assert!(!out.contains("\\ "), "indent became an escape: {out:?}");
    assert_eq!(carve::to_html(&out), carve::to_html(src));
}

/// An escape inside an INLINE EXTENSION was invisible to the minimal/conservative
/// comparison, so it escalated the whole document to conservative.
#[test]
fn an_inline_extension_does_not_escalate_the_document() {
    let src = "Press :kbd[Ctrl+C] to copy.\n";
    assert_eq!(carve::to_carve(src), src);
    assert_eq!(carve::to_carve(":foo[a [b] c]\n"), ":foo[a [b] c]\n");
}

/// An escape that IS needed inside an inline extension must still survive -- the
/// fix must not turn escalation off wholesale.
#[test]
fn a_needed_escape_inside_an_extension_survives() {
    let src = "Press :kbd[a \\-\\- b] now.\n";
    let out = carve::to_carve(src);
    assert!(out.contains("\\-\\-"), "escape was dropped: {out:?}");
    assert_eq!(carve::to_html(&out), carve::to_html(src));
    assert_eq!(carve::to_carve(&out), out, "fmt is not idempotent");
}

/// The caption slot is a SPACE after the marker. A tab leaves the line as prose,
/// which corpus 231 (a tab after a heading, quote or caption marker leaves the
/// line as prose) pins on exactly this document, so the caret re-parses as text
/// either way and PART 11 section 4 asks for the minimal form when dropping the
/// escape changes nothing. This writer forced it, emitting a backslash where
/// carve-js emits the bare caret. That corpus case has no `.fmt` fixture, so only
/// the cross-engine render comparison could see it.
#[test]
fn a_tab_after_the_caret_is_not_a_caption_slot() {
    let source = "![Moon](m.jpg)\n^\tFigure 1\n";
    let out = carve::to_carve(source);

    assert_eq!(out, source);
    assert_eq!(carve::to_html(&out), carve::to_html(source));
    assert_eq!(carve::to_carve(&out), out, "fmt is not idempotent");
}
