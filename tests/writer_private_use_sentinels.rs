//! `fmt` must not rewrite the characters an author wrote.
//!
//! `restore_verbatim` was four GLOBAL `replace` calls, which cannot tell a
//! sentinel the writer inserted from one the author wrote. An authored U+E003 was
//! deleted, U+E004 became a space, and the staged pair U+E011/U+E012 became a
//! space and a tab.
//!
//! Measured before the fix: the character was lost in 16 of 17 constructs, not
//! just in a code block as carve-rs#607 was titled. Only frontmatter survived,
//! being emitted outside the normalization path.
//!
//! Two of the four are inserted in exactly ONE position, so the restore now
//! undoes those two only there: VERBATIM_BLANK as a whole line, and U+E004 as a
//! line PREFIX. No traversal and no bookkeeping.
//!
//! TWO KINDS OF RESIDUE, both stated rather than hidden.
//!
//! An authored U+E003 alone on its own line, or a U+E004 at a line start, still
//! collides - those are exactly the positions the writer uses. Closing that needs
//! the insertion COUNTS, which is the design sketched on carve-rs#607.
//!
//! And the STAGED PAIR (U+E011/U+E012) is untouched by this change, so an
//! authored one is still rewritten to a space or a tab. It has TWO insertion
//! positions rather than one: `protect_verbatim` stages a line's TRAILING run,
//! and the line-block layout path stages a LEADING or MEDIAL run. I made that
//! half positional too on the first attempt and `line_block_medial_gaps` failed -
//! the medial case was being dropped. Separate sentinels for the two purposes
//! would let it be positional; that is a different change.

use carve::{parse, render_carve};

fn fmt(src: &str) -> String {
    render_carve(&parse(src)).expect("render")
}

/// Every construct the sweep on carve-rs#607 covered, minus frontmatter, which
/// never went through the normalization path.
fn constructs(ch: char) -> Vec<(&'static str, String)> {
    let c = ch.to_string();
    vec![
        ("paragraph", format!("text a{c}z here\n")),
        ("code block", format!("```\na{c}z\n```\n")),
        ("inline code", format!("a `x{c}y` b\n")),
        ("heading", format!("# H{c}X\n")),
        ("list item", format!("- item{c}x\n")),
        ("block quote", format!("> quoted{c}x\n")),
        ("table cell", format!("| a{c}b | c |\n")),
        ("link text", format!("[te{c}xt](/u)\n")),
        ("link destination", format!("[t](/u{c}v)\n")),
        ("image alt", format!("![al{c}t](/p.png)\n")),
        ("emphasis", format!("/em{c}ph/\n")),
        ("footnote body", format!("[^a]: no{c}te\n\nsee[^a]\n")),
        ("raw block", format!("```=html\n<b>a{c}z</b>\n```\n")),
        ("div", format!(":::\nbo{c}dy\n:::\n")),
        ("attribute value", format!("{{key=\"a{c}z\"}}\ntext\n")),
        ("definition list term", format!(":: te{c}rm\n:  def\n")),
    ]
}

/// The CHARACTER surviving is the claim, not byte equality of the whole document.
/// Byte equality over-reports: the writer legitimately drops unneeded quotes
/// around an attribute value and moves a footnote definition to the end, and
/// neither touches the character.
fn survives_everywhere(ch: char) -> Vec<&'static str> {
    constructs(ch)
        .into_iter()
        .filter(|(_, src)| !fmt(src).contains(ch))
        .map(|(name, _)| name)
        .collect()
}

#[test]
fn an_authored_verbatim_blank_sentinel_survives_every_construct() {
    assert_eq!(survives_everywhere('\u{e003}'), Vec::<&str>::new());
}

#[test]
fn an_authored_no_column_zero_marker_survives_every_construct() {
    assert_eq!(survives_everywhere('\u{e004}'), Vec::<&str>::new());
}

#[test]
fn the_sentinels_still_do_their_job() {
    // What each one exists for. If the positional restore missed its own
    // insertion site, these would break instead.
    //
    // A blank line inside a code block: without VERBATIM_BLANK the
    // whole-document trim eats it.
    let blank = "```\na\n\nb\n```\n";
    assert_eq!(
        fmt(blank),
        blank,
        "a blank line inside a code block was lost"
    );

    // Trailing whitespace inside a code block: what the staged pair protects.
    let trailing = "```\na \t\nb\n```\n";
    assert_eq!(
        fmt(trailing),
        trailing,
        "trailing whitespace inside a code block was lost"
    );
}

#[test]
fn a_thematic_break_still_round_trips() {
    // A weaker claim than it first looks, and labelled as such.
    //
    // I wrote this to cover the U+E004 guard - a PARAGRAPH whose rendered text is
    // a run of dashes, which the writer prefixes so it does not come back as a
    // thematic break. `---` alone is a genuine thematic break, so this does NOT
    // reach that path: it passes with or without the guard.
    //
    // I could not find an input that reaches it through the default writer at
    // all. `\---` and `\-\-\-` come back ESCAPED rather than space-prefixed, and a
    // literal em dash is emitted as itself. The guard appears to need
    // smart-typography source mode, which this CLI path does not select. Left as
    // a round-trip invariant rather than deleted, with the gap recorded so nobody
    // reads it as coverage of U+E004's insertion site.
    let src = "---\n";
    assert_eq!(to_html(&fmt(src)), to_html(src));
}

fn to_html(src: &str) -> String {
    carve::to_html(src)
}
