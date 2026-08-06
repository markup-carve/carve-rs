//! The frontmatter opener's format slot is a space.
//!
//! `frontmatter_open = "---", [space], [frontmatter_format], newline`. The slot
//! before the format token is PADDING - the `---` pair has already decided the
//! block, and the token only names the metadata dialect - but PART 7's MARKER
//! SEPARATORS AND PADDING SLOTS decides the terminal by POSITION rather than by
//! role: the slot sits after the first non-whitespace character of the line, and
//! a tab is syntax only inside a line's leading indentation run. So
//! `---<TAB>yaml` is not a typed opener at all; it is an ordinary line, and the
//! lines under it are ordinary blocks (carve#901, landed as carve#905).
//!
//! ONE CASE PER DIRECTION. The rule is about a RUN, so a check on the run's
//! first character is not a check on the rule: it rejects `---<TAB>yaml` while
//! `---<SP><TAB>yaml` still opens a block. That exact shape survived
//! carve-rs#720 and was only found in carve-rs#722, so both directions are
//! watched here.
//!
//! TWO PRODUCERS. `split_frontmatter` (the parse path) and `raw_frontmatter`
//! (the `fmt` path) each carried their own copy of the opener test. A `fmt` that
//! disagrees with the parser about what a frontmatter block is would rewrite an
//! ordinary line into one, so both are asserted.

use carve::parse;

fn has_frontmatter(source: &str) -> bool {
    parse(source).frontmatter_raw.is_some()
}

/// The first line `fmt` writes.
///
/// `raw_frontmatter` rebuilds the opener as `---` glued to the format token it
/// read, so a `fmt` that saw a typed opener where the parser sees none announces
/// itself here - and it announces itself with the tab NORMALIZED AWAY, which is
/// what makes the disagreement a silent rewrite rather than a visible one.
fn fmt_first_line(source: &str) -> String {
    carve::to_carve(source)
        .lines()
        .next()
        .unwrap_or_default()
        .to_string()
}

fn assert_not_frontmatter(label: &str, source: &str, token: &str) {
    assert!(
        !has_frontmatter(source),
        "{label}: opened a frontmatter block"
    );
    assert_ne!(
        fmt_first_line(source),
        format!("---{token}"),
        "{label}: fmt wrote a typed frontmatter fence"
    );
}

fn assert_frontmatter(label: &str, source: &str, format: &str) {
    let raw = parse(source)
        .frontmatter_raw
        .unwrap_or_else(|| panic!("{label}: no frontmatter block"));
    assert_eq!(raw.format, format, "{label}: wrong format token");
    assert_eq!(
        fmt_first_line(source),
        format!("---{format}"),
        "{label}: fmt dropped the fence"
    );
}

#[test]
fn a_tab_does_not_pad_the_format_slot() {
    assert_not_frontmatter("tab first", "---\tyaml\na: 1\n---\nx\n", "yaml");
    assert_not_frontmatter("space then tab", "--- \tyaml\na: 1\n---\nx\n", "yaml");
    assert_not_frontmatter("tab then space", "---\t yaml\na: 1\n---\nx\n", "yaml");
}

#[test]
fn a_tab_alone_does_not_pad_the_format_slot() {
    // No token at all, just the run. A bare `---` IS an opener, so this case
    // isolates the run from the token.
    //
    // The parse decision only. `fmt` writes this one back as the thematic break
    // it now is, and a document that OPENS with a thematic break and carries a
    // later `---` re-reads as frontmatter on the next parse - measured on the
    // same build for a `***` opener, which this ticket does not touch. That is a
    // separate defect in the writer, not this slot, so asserting it here would
    // pin the wrong rule.
    assert!(
        !has_frontmatter("---\t\na: 1\n---\nx\n"),
        "tab, no token: opened a frontmatter block"
    );
}

#[test]
fn a_unicode_space_does_not_pad_the_format_slot_either() {
    // The slot was a full Unicode `str::trim`, so it admitted the whole
    // White_Space property and not only the tab. Narrowing the terminal to a
    // literal `' '` drops both; narrowing it to `[' ', '\t']` would have
    // re-admitted the tab, and narrowing it to "not a tab" would have left
    // these.
    assert_not_frontmatter("no-break space", "---\u{a0}yaml\na: 1\n---\nx\n", "yaml");
    assert_not_frontmatter("em space", "---\u{2003}yaml\na: 1\n---\nx\n", "yaml");
}

#[test]
fn the_lines_under_a_rejected_opener_are_ordinary_blocks() {
    // What the production says happens instead, rather than only what does not:
    // the metadata line is prose and the closing `---` is a thematic break.
    let out = carve::to_html("---\tyaml\na: 1\n---\nx\n");
    assert!(out.contains("a: 1"), "the metadata line vanished: {out}");
    assert!(out.contains("<hr>"), "no thematic break: {out}");
}

#[test]
fn a_space_still_pads_the_format_slot() {
    assert_frontmatter("one space", "--- yaml\na: 1\n---\nx\n", "yaml");
}

#[test]
fn the_slot_is_a_run_not_a_single_space() {
    // The production spells the slot `[space]`, exactly one character, while
    // every engine reads a run. Which side gives is a question for the
    // production; narrowing the terminal here must not decide it by accident.
    assert_frontmatter("two spaces", "---  toml\na = 1\n---\nx\n", "toml");
}

#[test]
fn the_canonical_and_bare_openers_are_unchanged() {
    assert_frontmatter("glued", "---toml\na = 1\n---\nx\n", "toml");
    // A bare fence defaults to yaml in the AST but writes back bare, so the
    // shared helper's fence assertion does not apply to it.
    let bare = parse("---\na: 1\n---\nx\n")
        .frontmatter_raw
        .expect("bare: no frontmatter block");
    assert_eq!(bare.format, "yaml");
    assert_eq!(fmt_first_line("---\na: 1\n---\nx\n"), "---");
}

#[test]
fn trailing_whitespace_after_the_token_is_still_tolerated() {
    // A different question - the line-ending rule, not this slot - and the spec
    // oracle tolerates it explicitly. Watched so that narrowing the leading run
    // is not quietly extended to the trailing one.
    assert_frontmatter("trailing tab", "---yaml\t\na: 1\n---\nx\n", "yaml");
    assert_frontmatter("trailing space", "---yaml \na: 1\n---\nx\n", "yaml");
}
