//! Hand-written rules follow the corrected catalog rows of carve#6333 (CC-1542, CC-1546, CC-1547, CC-1549).

fn ranges(input: &str, code: &str) -> Vec<(u32, u32)> {
    carve::lint::lint(input, &carve::lint::LintOptions::default())
        .into_iter()
        .filter(|finding| finding.code == code)
        .map(|finding| {
            let pos = finding.pos.expect("range");
            (pos.start_offset, pos.end_offset)
        })
        .collect()
}

#[test]
fn bare_html_tag_needs_the_catalog_tag_shape() {
    for input in [
        "[a <x]> b](u)\n",
        "a <x]> b\n",
        "a <x/y> b\n",
        "a <x\"> b\n",
    ] {
        assert!(ranges(input, "bare-html-tag").is_empty(), "{input:?}");
    }
    for (input, range) in [
        ("a <x> b\n", (2, 5)),
        ("a <x/> b\n", (2, 6)),
        ("a </x> b\n", (2, 6)),
        ("a <x y=\"]\"> b\n", (2, 11)),
        ("a <x-1:b_c\tz> b\n", (2, 13)),
    ] {
        assert_eq!(ranges(input, "bare-html-tag"), [range], "{input:?}");
    }
}

#[test]
fn caption_looking_paragraph_skips_term_text() {
    for input in [
        " :: term\n: | A | B |\n  | x | y |\n",
        "\t:: t\n: ## H #\n | 1 | 2 |\n",
        ":: term\nCaption: x\n| a |\n",
    ] {
        assert!(
            ranges(input, "caption-looking-paragraph").is_empty(),
            "{input:?}"
        );
    }
    assert_eq!(
        ranges("Caption: x\n| a | b |\n", "caption-looking-paragraph"),
        [(0, 1)]
    );
}

#[test]
fn github_alert_marker_skips_a_glued_first_quote_mark() {
    for input in [
        ">[x]: > [!NOTE]\n",
        ">* > [!NOTE]\n",
        ">- > [!NOTE]\n",
        "  >a > [!TIP]\n",
    ] {
        assert!(ranges(input, "github-alert-marker").is_empty(), "{input:?}");
    }
    for (input, start) in [
        ("> [!NOTE]\n", 2),
        (">[!NOTE]\n", 1),
        (">\t[!NOTE]\n", 2),
        ("> [x]: > [!NOTE]\n", 9),
    ] {
        let found = ranges(input, "github-alert-marker");
        assert_eq!(found.len(), 1, "{input:?}");
        assert_eq!(found[0].0, start, "{input:?}");
    }
}
