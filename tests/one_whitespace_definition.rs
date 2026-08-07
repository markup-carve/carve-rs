//! ONE WHITESPACE DEFINITION, IN EVERY CONSTRUCT (PART 7, markup-carve/carve#977).
//!
//! Carve has exactly four whitespace characters - U+0020, U+0009, U+000A and
//! U+000D. EVERY OTHER CHARACTER IS CONTENT, and the clause names the two an
//! implementation is likeliest to admit by accident so their absence cannot be
//! read as an oversight:
//!
//!   VERTICAL TAB (U+000B) is CONTENT.
//!   FORM FEED     (U+000C) is CONTENT.
//!
//! Rust hands out three wider classes and this engine had reached for all
//! three. `char::is_whitespace` is the Unicode White_Space property (it takes
//! the vertical tab, the form feed AND the no-break space); `str::trim` and
//! friends are built on it; `u8::is_ascii_whitespace` takes the form feed but
//! not the vertical tab. `trim_ascii`/`trim_ascii_start`/`trim_ascii_end` in
//! `parse.rs` are the local helpers that spell the production's set, and they
//! are what a line-classification site must use.
//!
//! ## The characters are built from escapes, never typed
//!
//! A literal U+000B in a source file is invisible and does not survive every
//! editor, every clipboard or every tool that touches the file. Three probe
//! files silently lost one while this rule was being measured across the
//! engines, and produced three wrong readings before a hexdump caught it. So
//! every case here builds the character with `char::from_u32` (or `\u{b}`) and
//! `the_probe_characters_are_the_ones_we_think_they_are` asserts the bytes
//! before any of it is believed.

const VT: char = '\u{b}';
const FF: char = '\u{c}';

fn html(src: &str) -> String {
    carve::to_html(src)
}

#[test]
fn the_probe_characters_are_the_ones_we_think_they_are() {
    // The guard the other tests stand on. If a rewrite ever turns one of these
    // into a space, this fails first and names the reason instead of letting
    // every case below quietly become a duplicate of its space control.
    assert_eq!(VT as u32, 0x0B);
    assert_eq!(FF as u32, 0x0C);
    assert_eq!(VT.to_string().as_bytes(), &[0x0B]);
    assert_eq!(FF.to_string().as_bytes(), &[0x0C]);
    // And the two Rust classes that disagree with the production, pinned so the
    // reason these tests exist stays visible in the file.
    assert!(VT.is_whitespace() && FF.is_whitespace());
    assert!(!(VT as u8).is_ascii_whitespace());
    assert!((FF as u8).is_ascii_whitespace());
}

/// Every case is the same shape: a construct whose recognition ends at a
/// whitespace run, with the run replaced by one control character. The
/// character is CONTENT, so the line is NOT that construct.
fn assert_not_the_construct(name: &str, template: &str, as_whitespace: &str) {
    for ch in [VT, FF] {
        let src = template.replace("{W}", &ch.to_string());
        let got = html(&src);
        assert_ne!(
            got, as_whitespace,
            "{name}: {ch:?} was read as whitespace, so the line was still the construct"
        );
        assert!(
            got.contains(ch),
            "{name}: {ch:?} did not survive into the output as content, got {got:?}"
        );
    }
}

/// The control for each case: the same template with a real space, which IS
/// whitespace and DOES leave the construct intact. Returns the rendered HTML so
/// the case above can assert it differs.
fn with_a_space(template: &str) -> String {
    html(&template.replace("{W}", " "))
}

#[test]
fn a_table_row_ends_at_its_closing_pipe() {
    let t = "| a |{W}\n";
    let control = with_a_space(t);
    assert!(control.contains("<table>"), "control: {control}");
    assert_not_the_construct("table row", t, &control);
}

#[test]
fn a_delimiter_row_ends_at_its_closing_pipe() {
    let t = "| h |\n|---|{W}\n| a |\n";
    let control = with_a_space(t);
    assert!(control.contains("<thead>"), "control: {control}");
    // A delimiter row that is not one is an ordinary row, so the table survives
    // and only the header does. The generic assertion wants the character in
    // the output, which it is - inside the cell.
    assert_not_the_construct("delimiter row", t, &control);
}

#[test]
fn a_container_opener_ends_at_its_type_word() {
    let t = "::: note{W}\nbody\n:::\n";
    let control = with_a_space(t);
    assert!(control.contains("admonition note"), "control: {control}");
    assert_not_the_construct("container opener", t, &control);
}

#[test]
fn a_line_block_opener_ends_at_its_pipe() {
    // `line_block_open = colon_fence, space, "|"` - nothing follows the pipe.
    // These two openers carried the wider class TWICE: the leading trim and a
    // second `trim_end` on the text after the fence. Narrowing only the first
    // left the second doing the same job, so both cases below still opened.
    let t = "::: |{W}\n  a\n:::\n";
    let control = with_a_space(t);
    assert!(control.contains("line-block"), "control: {control}");
    assert_not_the_construct("line block opener", t, &control);
}

#[test]
fn a_hardbreaks_opener_ends_at_its_backslash() {
    let t = "::: \\{W}\na\nb\n:::\n";
    let control = with_a_space(t);
    assert!(control.contains("hardbreaks"), "control: {control}");
    assert_not_the_construct("hardbreaks opener", t, &control);
}

#[test]
fn a_block_image_line_holds_nothing_after_the_image() {
    let t = "![alt](/i){W}\n";
    let control = with_a_space(t);
    assert_eq!(control, "<img src=\"/i\" alt=\"alt\">");
    assert_not_the_construct("block image", t, &control);
}

#[test]
fn a_block_attribute_line_holds_nothing_after_the_brace() {
    // The row the cross-engine comparison named. `opt_ws`, `attr_separator` and
    // `continuation` are all built from `whitespace`, so a vertical tab after
    // the closing brace is content and the line is an ordinary paragraph that
    // the next line folds into - it attaches to nothing.
    let t = "{#x}{W}\np\n";
    let control = with_a_space(t);
    assert_eq!(control, "<p id=\"x\">p</p>");
    for ch in [VT, FF] {
        let got = html(&t.replace("{W}", &ch.to_string()));
        assert!(
            !got.contains("id=\"x\""),
            "{ch:?} after the brace still attached the attributes: {got}"
        );
        assert!(got.contains(ch), "the character did not survive: {got:?}");
    }
}

#[test]
fn a_list_item_attribute_line_holds_nothing_after_the_brace() {
    // Braces ALONE on a list-item marker line are a block-attribute line, and
    // the discriminator is whether CONTENT FOLLOWS the braces. A vertical tab
    // is content, so it makes the line ordinary item text.
    let t = "- {a=b .c}{W}\n  text\n";
    let control = with_a_space(t);
    assert!(control.contains("class=\"c\""), "control: {control}");
    for ch in [VT, FF] {
        let got = html(&t.replace("{W}", &ch.to_string()));
        assert!(
            !got.contains("class=\"c\""),
            "{ch:?} after the brace still attached the attributes: {got}"
        );
        assert!(got.contains(ch), "the character did not survive: {got:?}");
    }
}

#[test]
fn a_frontmatter_opener_ends_at_its_format_token() {
    // The worst of the set: the opener runs to the next bare `---`, so reading
    // the character as whitespace does not merely mislabel one line - it
    // swallows every line down to the closer.
    let t = "---yaml{W}\nt: 1\n---\n\nb\n";
    let control = with_a_space(t);
    assert_eq!(control, "<p>b</p>");
    for ch in [VT, FF] {
        let src = t.replace("{W}", &ch.to_string());
        let got = html(&src);
        assert!(
            got.contains("t: 1"),
            "{ch:?} opened frontmatter and ate the document: {got:?}"
        );
        assert!(carve::parse(&src).frontmatter_raw.is_none());
    }
}

#[test]
fn an_abbreviation_expansion_keeps_what_is_not_whitespace() {
    // The expansion's trailing trim was `str::trim_end`, the Unicode property,
    // so it ate a trailing no-break space out of a title the author wrote. All
    // three characters are content and survive; a space and a tab are the
    // whitespace PART 2 drops.
    for ch in [VT, FF, '\u{a0}'] {
        let src = format!("*[HT]: Hyper{ch}\n\nHT\n");
        let got = html(&src);
        assert!(
            got.contains(&format!("title=\"Hyper{ch}\"")),
            "{ch:?} was trimmed out of the expansion: {got:?}"
        );
    }
    for ch in [' ', '\t'] {
        let src = format!("*[HT]: Hyper{ch}\n\nHT\n");
        assert_eq!(html(&src), "<p><abbr title=\"Hyper\">HT</abbr></p>");
    }
}

// ---------------------------------------------------------------------------
// The rows this engine already read correctly, kept as CONTROLS. Two of them
// are the rows a cross-engine comparison reported this engine as the outlier
// on, before PART 7 was written: the clause makes THIS the conformant answer
// and the other two engines the ones that move, so pinning them here is what
// keeps a future "align with the majority" sweep from undoing them.
// ---------------------------------------------------------------------------

#[test]
fn a_continuation_marker_takes_no_whitespace_at_all() {
    // CONTROL. `continuation_marker = '+', newline`, so ANY character between
    // the `+` and the line end is content and the line is not a marker. This
    // engine was reported as the outlier here; PART 7 names it the correct
    // reading.
    let control = html("- a\n+ \n  b\n");
    assert!(control.contains("<li>a"), "control: {control}");
    for ch in [VT, FF] {
        let got = html(&format!("- a\n+{ch}\n  b\n"));
        assert_ne!(got, control, "{ch:?} was accepted as a continuation marker");
        assert!(got.contains(ch), "the character did not survive: {got:?}");
    }
}

#[test]
fn a_marker_takes_one_space_and_what_follows_is_content() {
    // CONTROL. A bullet, an ordered item, a heading, a definition term, a block
    // quote and a caption each take ONE space after the marker; the character
    // after it is content, and MARKER REQUIRES CONTENT is satisfied by it. The
    // list-item and definition-term rows are the other two this engine was
    // reported as the outlier on.
    for (name, src) in [
        ("bullet", format!("- {VT}\n")),
        ("definition term", format!(":: {VT}\n:  d\n")),
        ("heading", format!("# {VT}\n")),
        ("caption", format!("| a |\n^ {VT}\n")),
        ("block quote", format!("> {VT}\n")),
    ] {
        let got = html(&src);
        assert!(
            got.contains(VT),
            "{name}: the vertical tab was not read as content: {got:?}"
        );
    }
}

#[test]
fn a_line_holding_one_control_character_is_not_blank() {
    // CONTROL. `blank_line = {whitespace}, newline`, so a line whose only
    // character is a vertical tab is not blank and does not separate blocks.
    let got = html(&format!("a\n{VT}\nb\n"));
    assert!(got.contains(VT), "{got:?}");
    assert_eq!(
        got.matches("<p>").count(),
        1,
        "the line separated blocks: {got:?}"
    );
}

#[test]
fn a_space_and_a_tab_are_still_whitespace() {
    // CONTROL for the whole file. Narrowing the class must not narrow it past
    // the production: both of the characters that ARE whitespace still are, at
    // every site the cases above moved.
    assert!(html("| a |\t\n").contains("<table>"));
    assert!(html("::: note \nbody\n:::\n").contains("admonition note"));
    assert_eq!(html("![alt](/i)\t\n"), "<img src=\"/i\" alt=\"alt\">");
    assert_eq!(html("{#x}\t\np\n"), "<p id=\"x\">p</p>");
    assert!(html("- {a=b .c} \n  text\n").contains("class=\"c\""));
    assert!(html("- {a=b .c}\t\n  text\n").contains("class=\"c\""));
    assert_eq!(html("---yaml \nt: 1\n---\n\nb\n"), "<p>b</p>");
    assert_eq!(html("---yaml\t\nt: 1\n---\n\nb\n"), "<p>b</p>");
    assert!(html("::: | \n  a\n:::\n").contains("line-block"));
    assert!(html("::: \\ \na\nb\n:::\n").contains("hardbreaks"));
}
