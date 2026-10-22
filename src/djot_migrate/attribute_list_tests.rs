use super::djot_to_carve;

#[test]
fn explicit_class_assignment_resets_classes_within_its_group() {
    for (source, expected) in [
        (
            "word{.a class=b .c}",
            "<p><span class=\"b c\">word</span></p>",
        ),
        (
            "word{.a class=b .c class=d .e}",
            "<p><span class=\"d e\">word</span></p>",
        ),
        (
            "word{.a title=x class=b .c}",
            "<p><span title=\"x\" class=\"b c\">word</span></p>",
        ),
        (
            "word{.a\n.md:flex}",
            "<p><span class=\"a md:flex\">word</span></p>",
        ),
        (
            "word{.a\n.-dash}",
            "<p><span class=\"a -dash\">word</span></p>",
        ),
        (
            "word{.a\n.md:flex class=b .c}",
            "<p><span class=\"b c\">word</span></p>",
        ),
        (
            "word{.a\nclass=b .md:flex}",
            "<p><span class=\"b md:flex\">word</span></p>",
        ),
        (
            "word{class=a}{class=b}",
            "<p><span class=\"a b\">word</span></p>",
        ),
    ] {
        assert_eq!(crate::to_html(&djot_to_carve(source)), expected);
    }
}

#[test]
fn opening_brackets_in_reference_definition_labels_resolve() {
    for (source, expected) in [
        (
            "[a[b]: /u\n\n[`a[b`][]",
            "<p><a href=\"/u\"><code>a[b</code></a></p>",
        ),
        ("[a[b]: /u\n\n[x][a\\[b]", "<p><a href=\"/u\">x</a></p>"),
        (
            "[a[b]: /u\n[b]: /v\n\n[a\\[b][]",
            "<p><a href=\"/u\">a[b</a></p>",
        ),
    ] {
        let result = crate::migration::migrate_djot(source);
        assert_eq!(crate::to_html(&result.value), expected);
        assert!(!result
            .report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("unresolved Djot reference")));
    }
}

#[test]
fn opening_bracket_reference_aliases_keep_contexts_and_avoid_collisions() {
    for source in [
        "[a[b]: /u\n\n| [x][a\\[b] |",
        "[a[b|c]: /u\n\n| [x][a\\[b\\|c] |",
        "[a[b]: /u\n\n> [x][a\\[b]",
        "[a[b]: /u\n\n- [x][a\\[b]",
        "[a[b]: /old\n[a[b]: /u\n\n[x][a\\[b]",
        "[carve-djot-reference-1]: /other\n[a[b]: /u\n\n[x][a\\[b] [y][carve-djot-reference-1]",
    ] {
        let native = djot_to_carve(source);
        let html = crate::to_html(&native);
        assert!(html.contains("<a href=\"/u\">x</a>"), "{source}: {html}");
        if source.contains("/other") {
            assert!(html.contains("<a href=\"/other\">y</a>"));
        }
    }
}

#[test]
fn shorthand_reference_classes_keep_their_append_semantics() {
    for (source, classes) in [
        ("{.a .md:flex}\n[r]: /u\n\n[x][r]", "a md:flex"),
        ("{.a\n  .md:flex}\n[r]: /u\n\n[x][r]", "a md:flex"),
        ("[r]: /u\n\n[x][r]{.a .md:flex}", "a md:flex"),
        ("{.a class=b .md:flex}\n[r]: /u\n\n[x][r]", "b md:flex"),
    ] {
        assert_eq!(
            crate::to_html(&djot_to_carve(source)),
            format!("<p><a href=\"/u\" class=\"{classes}\">x</a></p>"),
            "{source}"
        );
    }
}

#[test]
fn valid_attribute_lists_keep_their_delimiters() {
    for source in [
        "{#id .class}\nA paragraph\n",
        "{#id}\n> Block quote\n",
        "{#id}\n# Heading\n",
        "[nested [span]{.blue}]{#ident}\n",
    ] {
        assert_eq!(djot_to_carve(source), source, "{source}");
    }
    assert_eq!(
        crate::to_html(&djot_to_carve("[span]{title=\"_*#literal*\"}\n")),
        "<p><span title=\"_*#literal*\">span</span></p>"
    );
}

#[test]
fn multiline_attribute_lists_are_canonicalized() {
    assert_eq!(
        crate::to_html(&djot_to_carve(
            "{#id .class\n  style=\"color:red\"}\nA paragraph\n"
        )),
        "<p id=\"id\" class=\"class\" style=\"color:red\">A paragraph</p>"
    );
}

#[test]
fn rejected_attributes_do_not_hide_paragraph_boundaries_or_later_spans() {
    let source = "_a [x]{.a\n\n.b} b_";
    assert!(!djot_to_carve(source).contains("/a"));
    assert!(!crate::to_html(&djot_to_carve("para\n{#id}\nnext")).contains("id=\"id\""));
    assert!(
        crate::to_html(&djot_to_carve("a { \"q\n[x]{title=\"_a_\"}"))
            .contains("<span title=\"_a_\">x</span>")
    );
    assert_eq!(
        crate::to_html(&djot_to_carve("[x]{#a\n.b}")),
        "<p><span id=\"a\" class=\"b\">x</span></p>"
    );
    let invalid = "[x]{k=a*b*}";
    assert_eq!(super::mask_djot_attributes(invalid), invalid);
}

#[test]
fn attributes_in_code_and_invalid_lists_remain_literal() {
    for source in ["`[x]{#id}`", "```\n[x]{#id}\n```"] {
        let written = djot_to_carve(source);
        assert_eq!(crate::to_html(source), crate::to_html(&written));
    }
    assert_eq!(djot_to_carve("[span]{#a<b}"), "[span]\\{\\#a<b}");
    assert_eq!(djot_to_carve("a #tag"), "a \\#tag");
    assert_eq!(crate::to_html(&djot_to_carve("\\{#id}")), "<p>{#id}</p>");
}
