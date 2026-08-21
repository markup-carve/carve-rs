//! Regression for markup-carve/carve-rs#1214.
//!
//! Collecting a definition leaves `%%` plus a private-use code point on the
//! line it came from, and the parser answers `is_definition_placeholder`
//! POSITIONALLY - on a trimmed line, with no record of who wrote it. While the
//! suffix was FIXED at U+E005 / U+E006, a comment an author typed as `%%` plus
//! that code point reached every branch the collector's own marker reaches.
//!
//! Measured against the same document with an ordinary comment, before the fix:
//! the comment node vanished from the AST, the list item holding it came back
//! `- +` (emptied), a `:::` div came back empty, and an item's continuation line
//! was DEDENTED out of the item. Not a lost character - a block-structure
//! change, the same shape markup-carve/carve-js#1289 measured on the JS side.
//!
//! The suffix is now picked per document, the way the canonical writer picks its
//! own five (markup-carve/carve-rs#607, #630) and the way §11 N1a's list
//! boundary is picked (markup-carve/carve-rs#1210).

/// The two code points the parser prefers. An authored comment carrying either
/// must behave exactly like one carrying any other private-use character.
const CLAIMED: [char; 2] = ['\u{E005}', '\u{E006}'];

/// The control: a private-use code point no mechanism in the crate claims, so
/// it measures what an ordinary comment does.
const CONTROL: char = '\u{E015}';

/// Every shape whose parse the placeholder branches can reach. `{C}` is the
/// comment's single-character content.
const SHAPES: &[(&str, &str)] = &[
    ("top level alone", "%%{C}\n"),
    ("between paragraphs", "a\n\n%%{C}\n\nb\n"),
    ("interrupting a paragraph", "a\n%%{C}\nb\n"),
    ("a list item's content", "- %%{C}\n- second\n"),
    (
        "a list item's continuation line",
        "- item\n  %%{C}\n  tail\n",
    ),
    (
        "a continuation line above a dedent",
        "- item\n  %%{C}\ntail\n",
    ),
    ("a nested marker line", "* * %%{C}\n  next\n"),
    ("inside a block quote", "> a\n> %%{C}\n> b\n"),
    ("inside a div", ":::\n%%{C}\n:::\n"),
    (
        "a footnote definition's body",
        "[^f]: note\n\n  %%{C}\n\ntext[^f]\n",
    ),
    ("inside a code fence", "```\n%%{C}\n```\n"),
];

fn shape(template: &str, content: char) -> String {
    template.replace("{C}", &content.to_string())
}

/// Rewrite the payload code point to one spelling, so two renders of the same
/// shape differ only where the STRUCTURE differs.
fn canonical(rendered: &str, content: char) -> String {
    rendered.replace(content, "\u{2603}")
}

#[test]
fn a_claimed_code_point_reads_as_an_ordinary_comment_in_every_target() {
    for (name, template) in SHAPES {
        let control_source = shape(template, CONTROL);
        let expected: Vec<(&str, String)> = vec![
            ("html", canonical(&carve::to_html(&control_source), CONTROL)),
            (
                "markdown",
                canonical(&carve::to_markdown(&control_source), CONTROL),
            ),
            (
                "plain",
                canonical(&carve::to_plain_text(&control_source), CONTROL),
            ),
            ("ansi", canonical(&carve::to_ansi(&control_source), CONTROL)),
            (
                "json",
                canonical(&carve::to_json(&carve::parse(&control_source)), CONTROL),
            ),
            (
                "carve",
                canonical(&carve::to_carve(&control_source), CONTROL),
            ),
        ];

        for claimed in CLAIMED {
            let source = shape(template, claimed);
            let actual: Vec<(&str, String)> = vec![
                ("html", canonical(&carve::to_html(&source), claimed)),
                ("markdown", canonical(&carve::to_markdown(&source), claimed)),
                ("plain", canonical(&carve::to_plain_text(&source), claimed)),
                ("ansi", canonical(&carve::to_ansi(&source), claimed)),
                (
                    "json",
                    canonical(&carve::to_json(&carve::parse(&source)), claimed),
                ),
                ("carve", canonical(&carve::to_carve(&source), claimed)),
            ];

            for ((target, want), (_, got)) in expected.iter().zip(actual.iter()) {
                assert_eq!(
                    want,
                    got,
                    "U+{:04X} does not read as a comment in {name} ({target})",
                    u32::from(claimed)
                );
            }
        }
    }
}

#[test]
fn a_claimed_code_point_leaves_the_item_holding_it_non_empty() {
    // The sharpest single reading: the writer's emptied-item branch fires only
    // for an item with no children, so `- +` here means the comment node was
    // never built.
    for claimed in CLAIMED {
        let source = format!("- %%{claimed}\n- second\n");
        // The canonical form separates `%%` from its content (PART 11), so the
        // expectation carries that space; what matters is that the item still
        // holds a comment instead of coming back `- +`.
        assert_eq!(
            carve::to_carve(&source),
            format!("- %% {claimed}\n- second\n"),
            "an authored U+{:04X} comment emptied its list item",
            u32::from(claimed)
        );
    }
}

#[test]
fn a_claimed_code_point_does_not_dedent_the_line_below_it() {
    for claimed in CLAIMED {
        let source = format!("- item\n  %%{claimed}\n  tail\n");
        let formatted = carve::to_carve(&source);
        assert!(
            formatted.contains("\n  tail"),
            "an authored U+{:04X} comment dedented the continuation line out of its item: {formatted:?}",
            u32::from(claimed)
        );
        assert_eq!(
            carve::to_html(&formatted),
            carve::to_html(&source),
            "to_html(fmt(x)) != to_html(x) for an authored U+{:04X} comment",
            u32::from(claimed)
        );
    }
}

#[test]
fn collection_still_works_when_the_document_occupies_the_preferred_pair() {
    // The other direction: picking a different pair must not break the mechanism
    // the pair exists for. The definition still hoists out of the item, the
    // reference still resolves, and the comment survives.
    let source = format!(
        "- [d]: /u\n  %%{}%%{}\n\n[use][d]\n",
        CLAIMED[0], CLAIMED[1]
    );
    let rendered = carve::to_html(&source);
    assert!(
        rendered.contains("href=\"/u\""),
        "the definition stopped resolving once the document occupied the preferred pair: {rendered}"
    );
    assert_eq!(
        carve::to_html(&carve::to_carve(&source)),
        rendered,
        "to_html(fmt(x)) != to_html(x) once the document occupied the preferred pair"
    );
}

#[test]
fn the_scan_walks_past_a_long_occupied_prefix() {
    // A pick that stepped a PAIR at a time, or gave up at the first occupied
    // code point, would fall back to the preferred pair here and re-open the
    // collision. Every code point from the start of the pool through the
    // preferred pair is occupied, so the pick has to walk.
    let occupied: String = (0xe001..=0xe010u32)
        .map(|code| char::from_u32(code).expect("private-use code point"))
        .collect();
    let source = format!("- item\n  %%{}\n  tail\n\n`{occupied}`\n", CLAIMED[0]);
    let formatted = carve::to_carve(&source);
    assert!(
        formatted.contains("\n  tail"),
        "the pick gave up and the continuation line was dedented: {formatted:?}"
    );
    assert_eq!(
        carve::to_html(&formatted),
        carve::to_html(&source),
        "to_html(fmt(x)) != to_html(x) with a long occupied prefix"
    );
}

#[test]
fn a_document_occupying_the_whole_pool_still_parses() {
    // The exhaustion path: with no free pair anywhere the pick falls back to the
    // preferred pair, the behavior the parser had before it was picked at all -
    // markup-carve/carve-js#1289's `pickSentinelRun` lands in the same place. It
    // must terminate and render, not loop or panic.
    let occupied: String = (0xe001..=0xf8ffu32)
        .map(|code| char::from_u32(code).expect("private-use code point"))
        .collect();
    let source = format!("- [d]: /u\n\n[use][d]\n\n`{occupied}`\n");
    let rendered = carve::to_html(&source);
    assert!(
        rendered.contains("href=\"/u\""),
        "the definition stopped resolving with the pool exhausted: {rendered}"
    );
}
