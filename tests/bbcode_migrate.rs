use carve::{bbcode_to_carve, BbcodeImportError, BBCODE_MAX_INPUT_LENGTH};

#[test]
fn common_bbcode_vocabulary_matches_the_other_importers() {
    for (source, expected) in [
        ("[b]x[/b]", "*x*\n"),
        ("[i]x[/i]", "/x/\n"),
        ("[u]x[/u]", "_x_\n"),
        ("[s]x[/s]", "~x~\n"),
        ("[url=https://e.test]x[/url]", "[x](https://e.test)\n"),
        ("[url]https://e.test[/url]", "<https://e.test>\n"),
        ("[email]a@b.test[/email]", "<mailto:a@b.test>\n"),
        ("[img]x.png[/img]", "![](x.png)\n"),
        ("[code][b]x[/b][/code]", "```\n[b]x[/b]\n```\n"),
        ("[c][b]x[/b][/c]", "`[b]x[/b]`\n"),
        ("[quote]x[/quote]", "> x\n"),
        ("[quote=Alice]x[/quote]", "> x\n^ Alice\n"),
        ("[list][*]a[*]b[/list]", "- a\n- b\n"),
        ("[list=1][*]a[*]b[/list]", "1. a\n2. b\n"),
        ("[size=2]x[/size]", "x\n"),
        ("[color=red]x[/color]", "x\n"),
        ("[font=serif]x[/font]", "x\n"),
        ("[center]x[/center]", "x\n"),
        ("[hr]", "---\n"),
        (
            "[spoiler=t]x[/spoiler]",
            "{title=\"t\"}\n::: spoiler\nx\n:::\n",
        ),
        (
            "[youtube]abc_1[/youtube]",
            "![YouTube Video](https://www.youtube.com/watch?v=abc_1)\n",
        ),
        ("E=mc[sup]2[/sup]", "E=mc{^2^}\n"),
        ("H[sub]2[/sub]O", "H{,2,}O\n"),
        (
            "[table][tr][th]h[/th][/tr][tr][td]x[/td][/tr][/table]",
            "|= h |\n| x |\n",
        ),
        ("[noparse]*x*[/noparse]", "\\*x*\n"),
    ] {
        assert_eq!(bbcode_to_carve(source).unwrap(), expected, "{source}");
    }
}

#[test]
fn code_language_cannot_create_a_raw_html_block() {
    assert_eq!(
        bbcode_to_carve("[code==html]<b>x</b>[/code]").unwrap(),
        "```html\n<b>x</b>\n```\n"
    );
}

#[test]
fn input_is_normalized_and_bounded() {
    assert_eq!(bbcode_to_carve("a\0b\r\n").unwrap(), "a\u{fffd}b\n");
    assert!(matches!(
        bbcode_to_carve(&"a".repeat(BBCODE_MAX_INPUT_LENGTH + 1)),
        Err(BbcodeImportError::InputTooLarge { .. })
    ));
}

#[test]
fn nested_and_unclosed_containers_are_consumed_linearly() {
    assert_eq!(
        bbcode_to_carve("[quote][quote]x[/quote][/quote]").unwrap(),
        "> > x\n"
    );
    assert_eq!(bbcode_to_carve("[quote]unclosed").unwrap(), "> unclosed\n");
    assert_eq!(
        bbcode_to_carve("[list][*]a[list][*]b[/list][*]c[/list]").unwrap(),
        "- a\n  - b\n- c\n"
    );
    assert_eq!(bbcode_to_carve("[list][*]a").unwrap(), "- a\n");
}

#[test]
fn forum_quote_metadata_becomes_a_carve_attribution() {
    assert_eq!(
        bbcode_to_carve("[quote=\"9\" name=\"Alice\" date=\"d\" time=\"t\"]x[/quote]").unwrap(),
        "> x\n^ Alice (d t) #9\n"
    );
}

#[test]
fn a_post_that_occupies_every_sentinel_is_refused() {
    let source: String = (0xe001..=0xf8fe).filter_map(char::from_u32).collect();
    assert_eq!(
        bbcode_to_carve(&source),
        Err(BbcodeImportError::SentinelSpaceExhausted)
    );
}
