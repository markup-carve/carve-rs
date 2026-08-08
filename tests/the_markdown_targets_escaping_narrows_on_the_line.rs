//! THE MARKDOWN TARGET'S ESCAPING NARROWS ON THE LINE (PART 11 §8a, the ruling
//! on markup-carve/carve#970; carve-rs#824).
//!
//! M1 is not one rule across the metacharacter set. It splits three ways:
//!
//!   M1a THE ASTERISK KEEPS M1 UNCONDITIONALLY.
//!   M1b `_`, `#` AND `[` ARE ESCAPED IF AND ONLY IF the character is ADJACENT
//!       on the EMITTED LINE to an unescaped delimiter of the same character.
//!   M1c NOTHING ELSE NARROWS.
//!
//! The two halves are separately provable ON PURPOSE, and the mutation pair the
//! sibling PRs used is the reason: making the asterisk conditional must kill
//! only the M1a cases, and making the other three unconditional must kill only
//! the M1b cases. A single-rule implementation cannot satisfy both, so a
//! mutation that kills nothing means the two rules have been collapsed into one.
//!
//! carve-rs is the LAST engine here. carve-js landed it in
//! `markup-carve/carve-js#906` and carve-php in `markup-carve/carve-php#1072`,
//! and the rows below are the ones measured across all three.

fn md(src: &str) -> String {
    carve::to_markdown(src).trim().to_string()
}

// ---------------------------------------------------------------------------
// M1a: the asterisk keeps M1 unconditionally.
// ---------------------------------------------------------------------------

/// A literal `*` reaching this target in a text node is escaped, whatever else
/// stands on the line - including when nothing does.
#[test]
fn a_lone_asterisk_is_escaped_with_no_delimiter_on_the_line() {
    assert_eq!(md("a * b"), "a \\* b");
    assert_eq!(md("plain * text"), "plain \\* text");
}

/// M1a is not M1b happening to hold. THE ASTERISK DIFFERS IN KIND: this writer
/// spells emphasis with `*`, so a literal asterisk is the character the line's
/// markup is made of. Dropping the escapes here merges them into the writer's
/// own delimiter run, and `*\*\**` becomes `****` - which a CommonMark reader
/// publishes as a thematic break rather than as emphasis holding two asterisks.
#[test]
fn an_asterisk_inside_emphasis_keeps_both_escapes() {
    assert_eq!(md("/\\*\\*/"), "*\\*\\**");
}

// ---------------------------------------------------------------------------
// M1b: not adjacent, so the escape protects nothing.
// ---------------------------------------------------------------------------

#[test]
fn a_lone_underscore_hash_or_bracket_is_emitted_bare() {
    assert_eq!(md("a _ b"), "a _ b");
    assert_eq!(md("a # b"), "a # b");
    assert_eq!(md("a [ b"), "a [ b");
}

/// The rows the fleet measurement calls out: a backslash inside an identifier
/// breaks exact-match search in the published document and protects nothing a
/// CommonMark reader would read differently.
#[test]
fn an_identifier_keeps_the_characters_the_author_typed() {
    assert_eq!(md("company_id"), "company_id");
    assert_eq!(md("C#"), "C#");
    assert_eq!(md("issue #123"), "issue #123");
}

/// THE SHARP ROW. The same line carries a real `_` delimiter pair AND a lone
/// `_`. The pair is not emitted as underscores at all - this writer spells
/// underline as `<u>` - so the lone one is adjacent to nothing and goes bare.
/// Both siblings answer the same way.
#[test]
fn a_line_holding_a_real_delimiter_pair_still_emits_the_lone_one_bare() {
    assert_eq!(md("a _b_ c and _ d"), "a <u>b</u> c and _ d");
}

// ---------------------------------------------------------------------------
// M1b: adjacent, so unescaping would merge two characters into one run.
// ---------------------------------------------------------------------------

/// Every Markdown reader this target answers to resolves a delimiter by RUN
/// LENGTH, so an escape sitting next to a live delimiter of the same character
/// holds a run boundary apart under all of them at once. That is the case M1b
/// keeps, and it is why the minimum is stated at this width.
#[test]
fn two_adjacent_narrowed_characters_keep_both_escapes() {
    assert_eq!(md("a __ b"), "a \\_\\_ b");
    assert_eq!(md("a ## b"), "a \\#\\# b");
    assert!(
        md("a [[x]] b").starts_with("a \\[\\[x"),
        "{}",
        md("a [[x]] b")
    );
}

/// Adjacency is tested on the EMITTED LINE and not on the node. The parser
/// splits `a__b` into more than one text node, so a per-node test cannot see
/// the neighbour at all.
#[test]
fn adjacency_is_seen_across_a_node_boundary() {
    assert_eq!(md("a__b"), "a\\_\\_b");
}

// ---------------------------------------------------------------------------
// M1c and M2: what this clause does not touch.
// ---------------------------------------------------------------------------

/// M1c: `]` and the backtick keep M1 as written. A wider narrowing is NOT
/// authorized by §8a - "not for a fourth character, and not for a laxer test on
/// these three" - so this is a CONTROL on the boundary of the clause.
#[test]
fn control_a_closing_bracket_still_takes_m1_unconditionally() {
    assert_eq!(md("a ] b"), "a \\] b");
    // The two halves of a reference are spelled differently on purpose now: the
    // opener narrows and the closer does not.
    assert_eq!(md("a [x] b"), "a [x\\] b");
}

/// M2: a character the AUTHOR escaped is an `escaped_text` node and is emitted
/// AS AN ESCAPE whatever the character, untouched by M1b. It used to take the
/// sentinel and lose its backslash to the old intraword rule, which was M1b
/// deciding a node M1 never governed.
#[test]
fn an_authored_escape_is_emitted_as_an_escape() {
    assert_eq!(md("a \\_ b"), "a \\_ b");
    assert_eq!(md("a \\# b"), "a \\# b");
    assert_eq!(md("company\\_id"), "company\\_id");
}

// ---------------------------------------------------------------------------
// The sentinel scheme, and the guard the sibling review caught.
// ---------------------------------------------------------------------------

/// The three sentinels are private-use characters, and author content carrying
/// one must not be read back as an escape this renderer emitted.
#[test]
fn author_supplied_sentinel_characters_never_reach_the_output() {
    for sentinel in ['\u{E004}', '\u{E005}', '\u{E006}'] {
        let out = md(&format!("a{sentinel}b"));
        assert!(
            !out.contains(sentinel),
            "U+{:04X}: {out:?}",
            sentinel as u32
        );
        assert_eq!(out, "ab");
    }
}

/// THE CONTROL GUARD MUST NOT NARROW, which is the P1 the sibling review caught
/// on the spec side. `strip_controls` drops every `Cc` character bar tab and
/// newline, NOT the non-whitespace C0 class: DEL (U+007F) and the C1 controls
/// have to keep going, because CSI (U+009B) and OSC (U+009D) are
/// single-character forms of the sequences PART 9 §25's terminal rule exists to
/// stop. This case fails if the guard is widened to let them through.
#[test]
fn control_the_control_guard_still_refuses_del_and_the_c1_controls() {
    for c in std::iter::once('\u{7f}')
        .chain((0x80u32..0xA0).map(|c| char::from_u32(c).expect("C1 is a char")))
    {
        let out = md(&format!("a{c}b"));
        assert!(
            !out.contains(c),
            "U+{:04X} reached the output: {out:?}",
            c as u32
        );
    }
}

/// CONTROL: a `_` inside a CODE SPAN is a region this renderer reproduces
/// byte-exact, so nothing here may rewrite it - which is why the decision is
/// made on the sentinel rather than on a `\_` in the assembled output.
#[test]
fn control_a_code_span_is_reproduced_byte_exact() {
    assert_eq!(md("`a_b`"), "`a_b`");
    assert_eq!(md("`a\\_b`"), "`a\\_b`");
}

/// CONTROL: the other targets are untouched by this clause. Only the Markdown
/// writer escapes at all.
#[test]
fn control_the_other_targets_do_not_escape() {
    assert_eq!(carve::to_plain_text("a _ b").trim(), "a _ b");
    assert!(carve::to_html("a _ b").contains("a _ b"));
}
