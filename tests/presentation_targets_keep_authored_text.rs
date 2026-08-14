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
    ("table caption", "|= H |\n| a |\n^ Table caption\n", "Table caption"),
    ("fence title", "``` js \"src/app.js\"\nlet a = 1\n```\n", "src/app.js"),
    ("grouping label", "``` js [Node]\na\n```\n", "Node"),
    ("image caption (control)", "![alt](i.png)\n^ Figure caption\n", "Figure caption"),
    (
        "listing caption (control)",
        "``` js\nlet a = 1\n```\n^ Listing caption\n",
        "Listing caption",
    ),
    ("admonition title (control)", "::: note \"Title\"\nbody\n:::\n", "Title"),
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

/// The caption goes on its own line under the table, the shape an image and a
/// listing caption already use on this target.
#[test]
fn the_table_caption_is_its_own_line() {
    assert_eq!(
        carve::to_markdown("|= H |\n| a |\n^ Table caption\n"),
        "| H |\n| --- |\n| a |\nTable caption\n"
    );
}

/// A table with no caption is unchanged: the line appears only where the author
/// wrote one.
#[test]
fn an_uncaptioned_table_is_unchanged() {
    assert_eq!(carve::to_markdown("|= H |\n| a |\n"), "| H |\n| --- |\n| a |\n");
}

/// A following block keeps its blank-line separation, so the caption cannot
/// swallow it.
#[test]
fn a_block_after_a_captioned_table_stays_separate() {
    assert_eq!(
        carve::to_markdown("|= H |\n| a |\n^ Cap\n\nafter\n"),
        "| H |\n| --- |\n| a |\nCap\n\nafter\n"
    );
}

/// The terminal joins the title and label to the rule line it already draws, so
/// a captioned fence still reads as one block rather than three.
#[test]
fn the_terminal_rule_carries_the_title_and_label() {
    let ansi = carve::to_ansi("``` js \"src/app.js\" [Node]\nlet a = 1\n```\n");
    let plain: String = strip_ansi(&ansi);
    assert!(
        plain.contains("┌── js src/app.js [Node]"),
        "got: {plain:?}"
    );
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
