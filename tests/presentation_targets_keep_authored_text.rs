//! docs/graceful-degradation.md states the floor as a MUST: a renderer may drop
//! a construct's INTERACTION but not its WORDS. "A reader of the Markdown export
//! should see every panel's heading. Losing the click is fine; losing the words
//! is not."
//!
//! Three kinds of authored text were dropped outright while the whole suite
//! stayed green, because nothing asserted that a caption or a fence header
//! reached the non-HTML targets at all. carve-js and carve-php dropped exactly
//! the same three, so the cross-engine comparison reported agreement rather than
//! a defect - agreement is not correctness (carve#1179).

/// The three losses, plus three controls that already held. The controls are
/// what make the losses a defect rather than a limitation of the targets: an
/// image caption and a listing caption survive the same target the table's
/// caption did not.
const CASES: &[(&str, &str, &str)] = &[
    (
        "table caption",
        "|= H |\n| a |\n^ Table caption\n",
        "Table caption",
    ),
    (
        "fence title",
        "``` js \"src/app.js\"\nlet a = 1\n```\n",
        "src/app.js",
    ),
    ("grouping label", "``` js [Node]\na\n```\n", "Node"),
    (
        "image caption (control)",
        "![alt](i.png)\n^ Figure caption\n",
        "Figure caption",
    ),
    (
        "listing caption (control)",
        "``` js\nlet a = 1\n```\n^ Listing caption\n",
        "Listing caption",
    ),
    (
        "admonition title (control)",
        "::: note \"Title\"\nbody\n:::\n",
        "Title",
    ),
];

/// Containment, not bytes: a renderer may keep changing HOW it presents these
/// and still be held to keeping them.
#[test]
fn authored_text_reaches_every_presentation_target() {
    for (name, src, authored) in CASES {
        for (target, out) in [
            ("markdown", carve::to_markdown(src)),
            ("plain", carve::to_plain_text(src)),
            ("ansi", carve::to_ansi(src)),
        ] {
            assert!(
                out.contains(authored),
                "{name}: the {target} target dropped {authored:?} - see \
                 docs/graceful-degradation.md, a target may drop interaction, never words. \
                 got: {out:?}"
            );
        }
    }
}

/// PART 11 §10e T2. The caption is body text under the table, separated by ONE
/// BLANK LINE. The blank line is not cosmetic: written directly after the last
/// row, `Table caption` is read by a GFM reader as another ROW, and the words
/// come back as a fabricated `<td>` that no reader and no parser can tell from
/// an authored cell. Surviving as the wrong thing is worse than not surviving.
#[test]
fn the_table_caption_is_body_text_after_a_blank_line() {
    assert_eq!(
        carve::to_markdown("|= H |\n| a |\n^ Table caption\n"),
        "| H |\n| --- |\n| a |\n\nTable caption\n"
    );
}

/// A table with no caption is unchanged: the line appears only where the author
/// wrote one.
#[test]
fn an_uncaptioned_table_is_unchanged() {
    assert_eq!(
        carve::to_markdown("|= H |\n| a |\n"),
        "| H |\n| --- |\n| a |\n"
    );
}

/// A following block keeps its blank-line separation, so the caption cannot
/// swallow it.
#[test]
fn a_block_after_a_captioned_table_stays_separate() {
    assert_eq!(
        carve::to_markdown("|= H |\n| a |\n^ Cap\n\nafter\n"),
        "| H |\n| --- |\n| a |\n\nCap\n\nafter\n"
    );
}

/// PART 11 §10e T1. The title and the label take a BOLD STANDALONE LINE each
/// above the block, title first - the rendering a fenced div's title and label
/// already get on this target. Joining them to the rule line was measured and
/// rejected: that line exists only when the fence has a LANGUAGE, so a titled
/// fence without one would have needed a header invented for it, and a fence
/// carrying both tokens would have needed a separator invented too.
#[test]
fn the_terminal_puts_the_title_and_label_above_the_block() {
    let ansi = carve::to_ansi("``` js \"src/app.js\" [Node]\nlet a = 1\n```\n");
    assert_eq!(
        strip_ansi(&ansi),
        "src/app.js\n\nNode\n\n┌── js \n  let a = 1\n"
    );
    // The tokens are bold, the way the div's already are.
    assert!(
        ansi.starts_with("\u{1b}[1msrc/app.js\u{1b}[0m\n\n\u{1b}[1mNode\u{1b}[0m\n\n"),
        "got: {ansi:?}"
    );
}

/// The language keeps the rule line to ITSELF. A fence carrying no title and no
/// label is unchanged, which is what makes the lines above a response to
/// authored text rather than new furniture on every code block.
#[test]
fn a_plain_fence_still_gets_only_its_rule_line() {
    assert_eq!(
        strip_ansi(&carve::to_ansi("``` js\nlet a = 1\n```\n")),
        "┌── js \n  let a = 1\n"
    );
}

/// A fence with a title but NO LANGUAGE gets no rule line invented for it: the
/// title takes its standalone line and nothing else appears. This is the shape
/// §10e names when it rejects folding the title into the header.
#[test]
fn a_title_on_a_language_less_fence_invents_no_rule_line() {
    assert_eq!(
        strip_ansi(&carve::to_ansi("``` \"src/app.js\"\nlet a = 1\n```\n")),
        "src/app.js\n\n  let a = 1\n"
    );
}

/// The two spellings of the caption rule the corpus does not reach.
///
/// A captioned table the PARSER produces is a `table` carrying its own caption.
/// A `figure` whose target is a table is the same construct arriving through the
/// AST-ingest path, from an engine that models it that way - and it took a
/// separator of `""`, so the caption was written with no line break at all. That
/// is a harder failure than the one §10e ruled: the words are fused INTO the
/// last data cell rather than merely following it. One rule, two spellings; the
/// fixtures pin only the first.
#[test]
fn a_figure_over_a_table_separates_its_caption_too() {
    let json = concat!(
        r#"{"type":"document","srcByteLength":0,"children":[{"type":"figure","#,
        r#""target":{"type":"table","rows":["#,
        r#"{"type":"table_row","cells":[{"type":"table_cell","header":true,"#,
        r#""children":[{"type":"text","value":"H"}]}]},"#,
        r#"{"type":"table_row","cells":[{"type":"table_cell","header":false,"#,
        r#""children":[{"type":"text","value":"a"}]}]}]},"#,
        r#""caption":[{"type":"text","value":"Fruit prices"}]}]}"#,
    );
    let doc = carve::from_json(json).expect("decode the figure AST");

    // Markdown: the same blank line the direct spelling now takes, and for the
    // same reason - `| a |Fruit prices` is not even a separate cell, it is text
    // welded to the row.
    assert_eq!(
        carve::render_markdown(&doc).expect("render markdown"),
        "| H |\n| --- |\n| a |\n\nFruit prices\n"
    );

    // Plain text: the caption follows on its own line, which is what a captioned
    // table already renders as on this target.
    assert_eq!(
        carve::render_plain_text(&doc).expect("render plain text"),
        "H\na\nFruit prices\n"
    );
}

/// An uncaptioned quote is unchanged.
#[test]
fn a_quote_without_an_attribution_is_unchanged() {
    assert_eq!(carve::to_markdown("> q\n"), "> q\n");
    assert_eq!(carve::to_plain_text("> q\n"), "\"q\"\n");
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            // Skip `[` .. terminating letter.
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}
