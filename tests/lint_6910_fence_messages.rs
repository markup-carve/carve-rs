use carve::lint::{LintOptions, lint};

const RAW_MESSAGE: &str = "These further words are not read as a title, label or attributes. The block opens without them. Use \"``` =html\" for the same block with only valid opener parts; put attributes on their own line above it.";

#[test]
fn raw_formats_use_the_further_words_catalog_message() {
    for opener in [
        "```=html x",
        "```=html\tfoo bar",
        "```=html~",
        "```=html \"t",
        "```=html \"t\" x",
    ] {
        let source = format!("{opener}\nx\n```\n");
        let findings: Vec<_> = lint(&source, &LintOptions::default())
            .into_iter()
            .filter(|d| d.code == "fence-title-syntax")
            .collect();
        assert_eq!(findings.len(), 1, "{opener:?}: {findings:?}");
        assert_eq!(findings[0].message, RAW_MESSAGE, "{opener:?}");
    }
}

#[test]
fn a_bare_fence_attribute_uses_the_further_words_message() {
    for (opener, end) in [
        ("```{.a}", 7),
        ("```{.a} extra", 13),
        ("```{.a} \"title\"", 15),
    ] {
        let source = format!("{opener}\nx\n```\n");
        let findings: Vec<_> = lint(&source, &LintOptions::default())
            .into_iter()
            .filter(|d| d.code == "fence-title-syntax")
            .collect();
        assert_eq!(findings.len(), 1, "{opener:?}: {findings:?}");
        assert_eq!(findings[0].pos.as_ref().map(|p| p.start_column), Some(4));
        assert_eq!(findings[0].pos.as_ref().map(|p| p.end_offset), Some(end));
        assert_eq!(
            findings[0].message,
            "These further words are not read as a title, label or attributes. The block opens without them. Use \"```\" for the same block with only valid opener parts; put attributes on their own line above it."
        );
    }
}

#[test]
fn a_glued_attribute_after_a_parsed_word_fills_the_fallback_catalog_message() {
    for (opener, expected) in [
        (
            "```js{.a}",
            "A code or raw fence carries no inline attributes, so \"{.a}\" after its language or format is not attached. The block opens without them. Put \"{.a}\" on its own line directly above \"```js\".",
        ),
        (
            "```=html{.a}",
            "A code or raw fence carries no inline attributes, so \"{.a}\" after its language or format is not attached. The block opens without them. Put \"{.a}\" on its own line directly above \"```=html\".",
        ),
        (
            "::: note{.a}",
            "A div carries no inline attributes, so \"{.a}\" after its name is not attached. The block opens without them. Put \"{.a}\" on its own line directly above \"::: note\".",
        ),
    ] {
        let source = format!("{opener}\nx\n```\n");
        let findings: Vec<_> = lint(&source, &LintOptions::default())
            .into_iter()
            .filter(|d| d.code == "fence-opener-fallback")
            .collect();
        assert_eq!(findings.len(), 1, "{opener:?}: {findings:?}");
        assert_eq!(findings[0].message, expected, "{opener:?}");
    }
}
