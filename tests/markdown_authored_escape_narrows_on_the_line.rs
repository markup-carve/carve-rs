//! THE MARKDOWN TARGET'S AUTHORED ESCAPE NARROWS TOO (PART 11 §8b).
//!
//! §8a narrowed M1 and left M2 as written, so an `escaped_text` node kept its
//! escape whatever the character and wherever it stood. §8b narrows M2 on the
//! same finding:
//!
//!   M2a AN escaped_text NODE IS EMITTED AS AN ESCAPE WHEN ITS CHARACTER COULD
//!       BE READ AS MARKUP AT THE POSITION THE WRITER HAS REACHED ON THE
//!       EMITTED LINE, or when it is one of the smart-punctuation triggers.
//!   M2b `#` IS READ AS MARKUP ONLY AT A LINE'S CONTENT POSITION.
//!   M2c NOTHING ELSE NARROWS.

fn md(source: &str) -> String {
    carve::to_markdown(source).trim_end().to_string()
}

/// M2a. A character this target's readers never read as markup is emitted
/// bare, at any position: these are Carve's own delimiters.
#[test]
fn an_authored_escape_of_an_inert_character_is_emitted_bare() {
    for (input, want) in [
        ("hi \\@user ok", "hi @user ok"),
        ("a \\{x b", "a {x b"),
        ("a \\^x b", "a ^x b"),
        ("a \\%x b", "a %x b"),
        ("a \\:x b", "a :x b"),
        ("a \\/x b", "a /x b"),
    ] {
        assert_eq!(md(input), want, "input {input:?}");
    }
}

/// M2b. The hash is decided by where it stands: an ATX heading opens at a
/// line's content position, on a run of one to six closed by a space, a tab or
/// the end of the line.
#[test]
fn an_authored_hash_is_decided_by_its_position() {
    for (input, want) in [
        ("a \\#y b", "a #y b"),
        ("issue \\#123 fixed", "issue #123 fixed"),
        ("C\\# is a language", "C# is a language"),
        ("Bau \\#64748b", "Bau #64748b"),
        ("see (\\#tag) there", "see (#tag) there"),
        // The position passes and the run does not: `#tag` is a paragraph in
        // CommonMark, which is why the test is spelled on the run.
        ("\\#tag rest", "#tag rest"),
        ("\\# heading", "\\# heading"),
    ] {
        assert_eq!(md(input), want, "input {input:?}");
    }
}

/// ESCAPING THE FIRST HASH OF A RUN IS SUFFICIENT, which is §8a M1e's argument
/// about the angle bracket one character over: a heading that cannot open needs
/// nothing done to the rest of its run.
#[test]
fn only_the_first_hash_of_a_run_keeps_its_escape() {
    assert_eq!(md("\\#\\#\\# heading"), "\\### heading");
}

/// BOUND: a run of seven is not a heading in any flavour, so it goes bare.
#[test]
fn a_run_of_seven_hashes_is_not_a_heading() {
    assert_eq!(md("\\#\\#\\#\\#\\#\\#\\# x"), "####### x");
}

/// M2c. Every character Markdown CAN read keeps M2 as written. The bracket in
/// particular keeps its escape at every position, which is what leaves §8a's
/// argument about the two link grammars standing.
#[test]
fn a_character_this_target_can_read_keeps_its_escape() {
    for (input, want) in [
        ("a \\*x* b", "a \\*x\\* b"),
        ("a \\[x](y) b", "a \\[x\\](y) b"),
        ("a \\_x_ b", "a \\_x_ b"),
    ] {
        assert_eq!(md(input), want, "input {input:?}");
    }
}

/// The smart-punctuation triggers §8 states M2 for. Not Markdown
/// metacharacters, so M1 never reached them, and a processor with substitution
/// on rewrites the TEXT rather than reading markup.
#[test]
fn a_smart_punctuation_trigger_keeps_its_escape() {
    assert_eq!(md("a \\-\\- b"), "a \\-\\- b");
}

/// BOUND: narrowing M2 does not reach a code span, where a backslash is content
/// this renderer reproduces byte-exact. The resolver decides on the sentinel
/// rather than on the emitted escape, which is what keeps it out.
#[test]
fn a_code_span_keeps_its_own_backslash() {
    assert_eq!(md("`a \\# b`"), "`a \\# b`");
}

/// M2b's POSITION IS AFTER THE CONTAINER PREFIX, at any depth or combination
/// (markup-carve/carve#1332). Column 0 is the content position only of a line
/// no container encloses.
///
/// Measuring from column 0 unconditionally is what made `> \# heading` emit
/// `> # heading` - which CommonMark reads back as a heading inside the quote,
/// so a plain round trip corrupted the document rather than merely reformatting
/// it (markup-carve/carve#1330).
#[test]
fn the_content_position_is_measured_past_every_container_prefix() {
    for (input, want) in [
        ("> \\# heading", "> \\# heading"),
        ("> > \\# deep", "> > \\# deep"),
        ("- \\# heading", "- \\# heading"),
        ("1. \\# heading", "1. \\# heading"),
        ("1) \\# heading", "1) \\# heading"),
        ("- [ ] \\# heading", "- [ ] \\# heading"),
        ("- [x] \\# heading", "- [x] \\# heading"),
        ("[^a]: \\# heading\n\ntext[^a]", "text[^a]"),
    ] {
        assert!(
            md(input).contains(want),
            "input {input:?} wanted {want:?}, got {:?}",
            md(input)
        );
    }
}

/// AND ONLY AT THAT POSITION. The prefix moves the content position; it does
/// not make the whole line one. Each of these is the same prefix as a row above
/// with the hash one step past where a heading could open, and each drops the
/// escape - which is the half that says the fix is a position rather than a
/// blanket "inside a container, keep everything".
#[test]
fn a_hash_past_the_content_position_still_drops_its_escape() {
    for (input, want) in [
        ("> C\\# is a language", "> C# is a language"),
        ("- C\\# is a language", "- C# is a language"),
        ("> issue \\#123 fixed", "> issue #123 fixed"),
        // The position passes and the RUN does not, behind a prefix exactly as
        // it does at column 0.
        ("- \\#tag rest", "- #tag rest"),
    ] {
        assert_eq!(md(input), want, "input {input:?}");
    }
}
