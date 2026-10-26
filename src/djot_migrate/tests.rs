#[test]
fn empty_task_items_keep_their_checkboxes() {
    for (source, expected) in [
        ("- [ ]\n", "- [ ] +\n"),
        ("- [ ] a\n- [x]\n", "- [ ] a\n- [x] +\n"),
        ("- [ ] a\n- [x] b\n", "- [ ] a\n- [x] b\n"),
        ("1. [ ]\n", "1. [ ]\n"),
    ] {
        assert_eq!(super::djot_to_carve(source), expected, "{source:?}");
    }
}

#[test]
fn nested_image_label_import_has_bounded_work() {
    let depth = 1600;
    let source = format!("{}x{}", "![".repeat(depth), "](u)".repeat(depth));
    super::IMAGE_LABEL_FULL_PARSES.with(|count| count.set(0));
    assert_eq!(super::djot_to_carve(&source), "![x](u)");
    super::IMAGE_LABEL_FULL_PARSES.with(|count| assert!(count.get() <= 2));
    assert!(super::simple_nested_image_label("![a](u) ![b](v)").is_none());
}
use super::*;

#[test]
fn image_reference_child_text() {
    let alt = cached_regex!(r#"<img\b[^>]*\balt="([^"]*)""#).unwrap();
    let href = cached_regex!(r#"<a\b[^>]*\bhref="([^"]*)""#).unwrap();
    let code = cached_regex!(r"(?s)<code\b[^>]*>(.*?)</code>").unwrap();
    let tags = cached_regex!(r"<[^>]*>").unwrap();
    let space = cached_regex!(r"\s+").unwrap();
    for fixture in [
        include_str!("../../tests/fixtures/djot-image-reference-child-text.json"),
        include_str!("../../tests/fixtures/djot-image-reference-hard-breaks.json"),
    ] {
        let cases: serde_json::Value = serde_json::from_str(fixture).unwrap();
        for case in cases.as_array().unwrap() {
            let source = case["source"].as_str().unwrap();
            let imported = djot_to_carve(source);
            let html = crate::to_html(&imported);
            let actual = alt
                .captures(&html)
                .map(|capture| capture[1].replace("&apos;", "'"));
            if let Some(expected) = case.get("text") {
                let text = tags.replace_all(&html, "");
                let text = space.replace_all(&text, " ");
                assert_eq!(Some(text.trim()), expected.as_str(), "{source}\n{imported}");
            }
            if let Some(expected) = case.get("code") {
                let actual_code = code.captures(&html).map(|capture| capture[1].to_owned());
                assert_eq!(
                    actual_code.as_deref(),
                    expected.as_str(),
                    "{source}\n{imported}"
                );
            }
            if let Some(expected) = case.get("href") {
                let actual_href = href.captures(&html).map(|capture| capture[1].to_owned());
                assert_eq!(
                    actual_href.as_deref(),
                    expected.as_str(),
                    "{}\n{source}\n{imported}",
                    case["name"]
                );
            }
            assert_eq!(
                actual.as_deref(),
                case["alt"]
                    .as_str()
                    .map(|text| text.replace("&apos;", "'"))
                    .as_deref(),
                "{}\n{source}\n{imported}",
                case["name"]
            );
        }
    }
}

#[test]
fn site_frontmatter_is_preserved_outside_the_djot_reader() {
    let source = "---\nkey: my_long_value\n---\n\nBody.\n";
    assert_eq!(djot_to_carve(source), source);
}

#[test]
fn multiple_footnotes_are_opaque_to_delimiter_conversion() {
    let source =
        "Here's one.[^foo] Here's two.[^bar]\n\n[^foo]: First note.\n\n[^bar]: Second note.\n";
    let carve = djot_to_carve(source);
    assert!(carve.contains("[^foo]"), "{carve}");
    assert!(carve.contains("[^bar]"), "{carve}");
    assert!(carve.contains("First note."), "{carve}");
    assert!(carve.contains("Second note."), "{carve}");
    assert!(!carve.contains("[{^"), "{carve}");
    assert!(!carve.contains("[^}"), "{carve}");
}

#[test]
fn an_unclosed_footnote_token_does_not_mask_later_lines() {
    assert_eq!(
        djot_to_carve("Unclosed [^ then _one_.\n\nLater _two_."),
        "Unclosed [\\^ then /one/.\n\nLater /two/."
    );
}

#[test]
fn a_definition_document_does_not_rewrite_code_that_looks_like_a_term() {
    let source = ": term\n\n  Body.\n\n```\n:: code\n```\n";
    let carve = djot_to_carve(source);
    assert!(carve.contains("```\n:: code\n```"), "{carve}");
    assert!(!carve.contains("```\n{loose}"), "{carve}");
}

#[test]
fn a_djot_definition_item_becomes_a_carve_definition_item() {
    let source = ": orange\n\n  A citrus fruit.\n";
    let carve = djot_to_carve(source);
    assert!(carve.contains("{loose}"), "{carve}");
    assert!(carve.contains(":: orange"), "{carve}");
    assert!(carve.contains(":  A citrus fruit."), "{carve}");
}

#[test]
fn definition_conversion_is_local_to_documents_with_links_and_raw_blocks() {
    let source = ": term\n\n  Body with [a link](/url).\n\n``` =latex\n\\alpha\n```\n";
    let carve = djot_to_carve(source);
    assert!(carve.contains(":: term"), "{carve}");
    assert!(carve.contains("[a link](/url)"), "{carve}");
    assert!(carve.contains("``` =latex\n\\alpha\n```"), "{carve}");
}

#[test]
fn djot_only_block_markers_take_carve_spellings() {
    assert_eq!(djot_to_carve("* * *"), "***");
    assert_eq!(djot_to_carve("      - - -"), "***");
    assert_eq!(djot_to_carve("(1) one\n(2) two"), "1. one\n2. two");
    assert_eq!(djot_to_carve("(a) one\n(b) two"), "a. one\nb. two");
    assert_eq!(djot_to_carve("- item\n\n  * * *"), "- item\n\n  ***");
}

#[test]
fn emphasis_becomes_the_carve_spelling() {
    assert_eq!(djot_to_carve("_em_ text"), "/em/ text");
}

#[test]
fn subscript_and_superscript_take_the_braced_form() {
    assert_eq!(djot_to_carve("H~2~O"), "H{,2,}O");
    assert_eq!(djot_to_carve("E=mc^2^"), "E=mc{^2^}");
}

#[test]
fn repeated_djot_delimiters_preserve_nested_nodes() {
    assert_eq!(djot_to_carve("**strong**"), "{*{*strong*}*}");
    assert_eq!(djot_to_carve("~~gone~~"), "{,{,gone,},}");
}

#[test]
fn highlight_is_already_the_same_form() {
    assert_eq!(djot_to_carve("{=marked=}"), "{=marked=}");
}

/// Djot reads any blank run between two markers as one list; Carve reads
/// three or more as a hard boundary. Passing the run through therefore
/// split a list the source never split.
#[test]
fn a_blank_run_before_a_sibling_marker_does_not_split_the_list() {
    assert_eq!(
        djot_to_carve("- apples\n\n\n\n\n- oranges\n"),
        "- apples\n\n- oranges\n"
    );
}

/// The boundary is not a top-level rule, so neither is the collapse.
#[test]
fn the_run_collapses_inside_an_item_too() {
    assert_eq!(
        djot_to_carve("- outer\n  - inner\n\n\n\n\n  - inner2\n"),
        "- outer\n  \\- inner\n\n  - inner2\n"
    );
}

/// Two blank lines are the loose separator in both languages, so there is
/// nothing to correct.
#[test]
fn a_shorter_run_is_left_alone() {
    assert_eq!(
        djot_to_carve("- apples\n\n\n- oranges\n"),
        "- apples\n\n\n- oranges\n"
    );
}

/// Djot's own way of writing two lists is a different marker, and it means
/// the same in Carve. Nothing about it is a false boundary.
#[test]
fn a_marker_change_still_separates_two_lists() {
    assert_eq!(
        djot_to_carve("- apples\n\n* oranges\n"),
        "- apples\n\n* oranges\n"
    );
}

/// The rewrite fires only where the two languages disagree, which needs a
/// list open above the run. After a paragraph the run says nothing in
/// either language and the author's layout is left as written.
#[test]
fn a_run_that_follows_no_list_keeps_its_lines() {
    assert_eq!(
        djot_to_carve("paragraph\n\n\n\n\n- apples\n"),
        "paragraph\n\n\n\n\n- apples\n"
    );
}

/// A run followed by an item's own indented content continues that item at
/// any length -- N1a closes nothing -- so both languages already agree.
#[test]
fn a_run_before_an_items_content_keeps_its_lines() {
    assert_eq!(
        djot_to_carve("- apples\n\n\n\n\n  still apples\n"),
        "- apples\n\n\n\n\n  still apples\n"
    );
}

/// N1a applies inside a container too, and there a blank line is written
/// `>`, so the run and the markers around it are read through the prefix.
#[test]
fn a_quoted_run_collapses_and_keeps_its_prefix() {
    assert_eq!(
        djot_to_carve("> 1. one\n>\n>\n>\n> 2. two\n"),
        "> 1. one\n>\n> 2. two\n"
    );
}

/// An item's own indentation is what says the line continues the item, and
/// inside a quote it sits right after the marker -- so the quote prefix is
/// stripped and the indentation is not.
#[test]
fn a_quoted_run_after_an_items_indented_content_still_collapses() {
    assert_eq!(
        djot_to_carve("> - apples\n>   more apples\n>\n>\n>\n> - oranges\n"),
        "> - apples\n>   more apples\n>\n> - oranges\n"
    );
}

/// A fence opened inside a quote holds its blank lines as content exactly
/// like an unquoted one.
#[test]
fn a_run_inside_a_quoted_fence_is_content() {
    let source = "> ```\n> - one\n>\n>\n>\n> - two\n> ```\n";
    assert_eq!(djot_to_carve(source), source);
}

/// A marker one quote level away separates nothing the run could join.
#[test]
fn a_run_and_a_marker_at_different_depths_are_left_alone() {
    let source = "> - apples\n>\n>\n>\n- oranges\n";
    assert_eq!(djot_to_carve(source), source);
}

/// Blank lines inside a fenced block are that block's content.
#[test]
fn a_run_inside_a_fence_is_content() {
    let source = "```\ncode\n\n\n\n- not a marker\n```\n";
    assert_eq!(djot_to_carve(source), source);
}

/// An intraword `_x_` CONVERTS, to the braced form. Djot emphasizes it, and
/// an author who wanted the literal characters had to escape them, so an
/// unescaped run is emphasis the source states rather than an identifier
/// the converter should protect.
#[test]
fn an_intraword_underscore_converts_to_the_braced_form() {
    assert_eq!(
        djot_to_carve("snake_case_name stays"),
        "snake{/case/}name stays"
    );
    assert_eq!(djot_to_carve("MAX_BUFFER_SIZE"), "MAX{/BUFFER/}SIZE");
    assert_eq!(djot_to_carve("a _x_ and y_z_w"), "a /x/ and y{/z/}w");
}

/// The other side, and what makes it safe: the escape survives, so an
/// author who did mean the identifier keeps it.
#[test]
fn an_escaped_intraword_underscore_is_left_alone() {
    assert_eq!(djot_to_carve("snake\\_case\\_name"), "snake\\_case\\_name");
}

/// BOUND: the word-bounded rule still emits the BARE form, and shapes that
/// are not an intraword pair at all are untouched. Removing the intraword
/// rule leaves every row here passing.
#[test]
fn the_surrounding_underscore_shapes_are_unchanged() {
    assert_eq!(djot_to_carve("a _x_ b"), "a /x/ b");
    assert_eq!(djot_to_carve("__init__"), "{/{/init/}/}");
    assert_eq!(djot_to_carve("_leading"), "\\_leading");
    assert_eq!(djot_to_carve("[t](/a_b_c)"), "[t](/a_b_c)");
}

#[test]
fn code_is_never_rewritten() {
    assert_eq!(
        djot_to_carve("`_no_ **no**` yes _yes_"),
        "`_no_ **no**` yes /yes/"
    );
    assert_eq!(
        djot_to_carve("``` js\n_no_\n```\n\n_yes_"),
        "``` js\n_no_\n```\n\n/yes/"
    );
}

#[test]
fn a_destination_is_never_rewritten() {
    assert_eq!(
        djot_to_carve("[t](http://e.com/_a_/b) and _yes_"),
        "[t](http://e.com/_a_/b) and /yes/"
    );
}

#[test]
fn a_pair_never_spans_a_paragraph_break() {
    assert_eq!(djot_to_carve("_a\n\nb_"), "\\_a\n\nb\\_");
    assert_eq!(djot_to_carve("_a\nb_"), "/a\nb/");
}

#[test]
fn an_escaped_delimiter_is_literal_text() {
    assert_eq!(djot_to_carve("\\_not em\\_"), "\\_not em\\_");
}

/// A tag is the one construct that is not a pair, so escaping an enclosing
/// brace cannot neutralize it. Djot has no hashtag - pandoc renders
/// `a #y b` as `<p>a #y b</p>` - so every `#word` became a Carve tag span
/// (carve-php#1191).
#[test]
fn a_hash_does_not_become_a_tag() {
    assert_eq!(djot_to_carve("a #y b"), "a \\#y b");
    assert_eq!(djot_to_carve("a #1 b"), "a \\#1 b");
    assert_eq!(djot_to_carve("{#y#} x"), "\\{\\#y#} x");
}

/// A heading is `#` plus a space and is shared with Djot; `a#y` is not a
/// tag either. Djot character-reference text freezes before Carve reads it.
#[test]
fn the_hash_negatives_stay_bare() {
    assert_eq!(djot_to_carve("# Heading"), "# Heading");
    assert_eq!(djot_to_carve("a#y b"), "a#y b");
    assert_eq!(djot_to_carve("a &#8212; b"), "a &\\#8212; b");
    assert_eq!(djot_to_carve("a &#x2014; b"), "a &\\#x2014; b");
}

#[test]
fn constructs_that_mean_the_same_are_untouched() {
    assert_eq!(djot_to_carve("{+add+} and {-cut-}"), "{+add+} and {-cut-}");
}

#[test]
fn nesting_of_different_families_composes() {
    assert_eq!(djot_to_carve("_a **b** c_"), "/a {*{*b*}*} c/");
}

/// Djot spells subscript braced as well as bare and means the same by each,
/// so the braced form converts too. It previously fell through untouched
/// and stayed a Carve STRIKETHROUGH, which is a different word.
#[test]
fn the_braced_subscript_converts_like_the_bare_one() {
    assert_eq!(djot_to_carve("{~y~} a"), "{,y,} a");
    assert_eq!(djot_to_carve("a{~b~}c"), "a{,b,}c");
    assert_eq!(djot_to_carve("{~y~}"), djot_to_carve("~y~"));
}

/// The braced superscript is spelled identically in both languages, so the
/// conversion is the identity. Without a rule claiming the range the bare
/// rule matched the `^x^` INSIDE the braces and wrapped it again, into
/// `{{^x^}}`.
#[test]
fn the_braced_superscript_is_not_wrapped_twice() {
    assert_eq!(djot_to_carve("{^x^} a"), "{^x^} a");
    // The inner `{,b,}` is literal text in Djot and a SUBSCRIPT in Carve,
    // so it is escaped rather than passed through. Rendering the result
    // gives Djot's `<sup>a{,b,}c</sup>` back.
    assert_eq!(djot_to_carve("{^a{,b,}c^} x"), "{^a\\{,b,}c^} x");
}

/// A `+` bullet is a list item in Djot and the CONTINUATION MARKER in
/// Carve, so leaving it turns the list into a paragraph.
#[test]
fn the_plus_bullet_becomes_a_dash() {
    assert_eq!(djot_to_carve("+ one\n+ two\n"), "- one\n- two\n");
    assert_eq!(djot_to_carve("+ a\n  + b\n"), "- a\n  \\+ b\n");
}

/// An empty Djot plus bullet needs an explicit empty Carve item.
/// A plus inside ordinary text stays literal.
#[test]
fn an_empty_plus_item_and_inline_plus_keep_their_roles() {
    assert_eq!(djot_to_carve("+\n"), "- +\n");
    assert_eq!(djot_to_carve("a + b\n"), "a + b\n");
}

#[test]
fn a_table_continuation_row_is_left_alone() {
    let source = "| a | b |\n|---|---|\n| one | x |\n+ continues here | y |\n";
    assert_eq!(table_continuation_lines(source), HashSet::from([4]));
    assert_eq!(djot_to_carve(source), source);
}

#[test]
fn a_plus_bullet_containing_a_pipe_still_becomes_a_dash() {
    assert_eq!(
        djot_to_carve("A paragraph.\n\n+ a bullet with a | pipe\n"),
        "A paragraph.\n\n- a bullet with a | pipe\n"
    );
}

/// Carve syntax that is ordinary text in Djot has to be escaped or the
/// conversion renders something the source never said. `%%` is the sharpest
/// case: Carve reads it as a line comment and the line DISAPPEARS.
#[test]
fn carve_syntax_that_is_plain_djot_text_is_escaped() {
    assert_eq!(djot_to_carve("%% not a comment\n"), "\\%% not a comment\n");
    assert_eq!(djot_to_carve("/slashes/ x\n"), "\\/slashes/ x\n");
    assert_eq!(djot_to_carve("=marked= x\n"), "\\=marked= x\n");
    assert_eq!(djot_to_carve("{,sub,} x\n"), "\\{,sub,} x\n");
}

/// Escaping the brace ALONE is not enough when the delimiter inside it has
/// a bare Carve form: `\\{/y/}` still renders `{<em>y</em>}`.
#[test]
fn a_braced_delimiter_with_a_bare_form_escapes_both() {
    assert_eq!(djot_to_carve("{/y/} x\n"), "\\{\\/y/} x\n");
}

/// BOUND, and the reason the rule above is delimiter-specific: `{=x=}` is a
/// highlight in BOTH languages, so its inner `=` is markup that must
/// survive. Escaping it would break the one braced form that already works.
#[test]
fn the_braced_highlight_keeps_its_inner_delimiter() {
    assert_eq!(djot_to_carve("{=marked=} x"), "{=marked=} x");
}

/// BOUND: escaping never reaches code, a destination, or an unpaired
/// delimiter. None of these move under any of the rules above.
#[test]
fn code_destinations_and_unpaired_delimiters_are_untouched() {
    assert_eq!(djot_to_carve("a `_x_` b\n"), "a `_x_` b\n");
    assert_eq!(djot_to_carve("ftp://x/ y\n"), "ftp://x/ y\n");
    assert_eq!(djot_to_carve("a/b/c\n"), "a/b/c\n");
    assert_eq!(djot_to_carve("```\n/code/\n```\n"), "```\n/code/\n```\n");
}

/// A run longer than two is NOT exempt: the comment opens on the first two
/// signs and the rest of the line disappears, so the first sign takes the
/// one escape that makes the whole run literal (carve-rs#2451). Every
/// spelling here, at both run lengths and on a line that is not the first.
#[test]
fn a_percent_run_longer_than_two_takes_one_escape_on_its_first_sign() {
    assert_eq!(djot_to_carve("a %%%c b\n"), "a \\%%%c b\n");
    assert_eq!(djot_to_carve("a %%%%c b\n"), "a \\%%%%c b\n");
    assert_eq!(djot_to_carve("%%%c b\n"), "\\%%%c b\n");
    assert_eq!(djot_to_carve("x\n%%%c y\n"), "x\n\\%%%c y\n");
    // BOUND: already escaped in the source, and never a second escape.
    assert_eq!(djot_to_carve("a \\%% c\n"), "a \\%% c\n");
    // BOUND: a lone sign and an intraword pair are not openers.
    assert_eq!(djot_to_carve("a % c\n"), "a % c\n");
    assert_eq!(djot_to_carve("100%%x\n"), "100%%x\n");
    // CONTROL: the two-sign case already escaped and does not move.
    assert_eq!(djot_to_carve("a %% c\n"), "a \\%% c\n");
    assert_eq!(djot_to_carve("a\n%% c\n"), "a\n\\%% c\n");
}
