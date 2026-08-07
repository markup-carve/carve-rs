//! The link-title padding slot is a space, in every form that shares it.
//!
//! `link_title = space, ('"', ..., '"') | space, ("'", ..., "'")`, and
//! `image_title = link_title`, and `reference_definition` reuses `link_title`
//! too. The slot is PADDING rather than a marker separator - a link is already a
//! link once its destination has been read, and a definition is already a
//! definition - but PART 7's MARKER SEPARATORS AND PADDING SLOTS decides the
//! terminal by POSITION rather than by role: a tab is syntax only inside a
//! line's leading indentation run, and an inline destination is about as far
//! from a leading indentation run as a slot gets (carve#901, landed as
//! carve#905). The executable grammar spells it `destTitle = titleSp+ (quoted |
//! squoted)` with `titleSp = " "`.
//!
//! The same clause names one more slot on the reference-definition line: the one
//! before its trailing `attributes`. Nobody had measured it. It was wrong the
//! same way, and its guard was the mirror of the one found in carve-rs#722 - it
//! tested only the character ADJACENT to the `{`, so a run holding a tab passed
//! as long as its last character was a space.
//!
//! ONE CASE PER FORM, PER DIRECTION. Four forms share the rule and each has its
//! own producer, so a fixture for one proves nothing about the others; and the
//! rule is about a RUN, so a check on the run's first (or last) character is not
//! a check on the rule.

fn html(source: &str) -> String {
    carve::to_html(source)
}

fn assert_no_title(label: &str, out: &str) {
    assert!(!out.contains("title="), "{label}: took a title: {out}");
}

// --- inline link -----------------------------------------------------------

#[test]
fn a_tab_does_not_pad_the_inline_link_title_slot() {
    // A run holding a tab leaves the parser short of the `)` the production
    // requires, so the construct fails and the text stays literal - which is
    // already what a U+00A0 in the same slot does.
    for (label, src) in [
        ("tab first", "[t](/u\t\"T\")\n"),
        ("space then tab", "[t](/u \t\"T\")\n"),
        ("tab then space", "[t](/u\t \"T\")\n"),
    ] {
        let out = html(src);
        assert_no_title(label, &out);
        assert!(!out.contains("<a "), "{label}: linked: {out}");
    }
}

#[test]
fn a_tab_does_not_pad_a_single_quoted_inline_link_title_either() {
    // Same slot, the other quote character: `link_title` has two alternatives
    // and they share the padding run.
    assert_no_title("single quotes", &html("[t](/u\t'T')\n"));
}

#[test]
fn a_tab_before_the_closing_paren_is_not_a_link_either() {
    // The same run with no title after it. `linkTail = "(" dest destTitle? ")"`
    // puts no slot between the destination and the `)`, so a tab there is not
    // syntax and the construct fails.
    let out = html("[t](/u\t)\n");
    assert!(!out.contains("<a "), "linked: {out}");
}

// --- inline image ----------------------------------------------------------

#[test]
fn a_tab_does_not_pad_the_inline_image_title_slot() {
    for (label, src) in [
        ("tab first", "![t](/u\t\"T\")\n"),
        ("space then tab", "![t](/u \t\"T\")\n"),
        ("tab then space", "![t](/u\t \"T\")\n"),
    ] {
        let out = html(src);
        assert_no_title(label, &out);
        assert!(!out.contains("<img"), "{label}: made an image: {out}");
    }
}

// --- reference definition, title slot --------------------------------------

#[test]
fn a_tab_does_not_pad_the_reference_definition_title_slot() {
    // Here a failed slot drops the TITLE rather than the whole construct: the
    // production tolerates trailing junk after the destination, so `[r]: /u x`
    // is a definition whose `x` is ignored, and the same is true of a tabbed
    // title.
    for (label, src) in [
        ("tab first", "[r]: /u\t\"T\"\n\n[t][r]\n"),
        ("space then tab", "[r]: /u \t\"T\"\n\n[t][r]\n"),
        ("tab then space", "[r]: /u\t \"T\"\n\n[t][r]\n"),
    ] {
        let out = html(src);
        assert_no_title(label, &out);
        assert!(
            out.contains("href=\"/u\""),
            "{label}: lost the definition: {out}"
        );
    }
}

#[test]
fn a_reference_image_takes_the_same_corrected_title() {
    // `![t][r]` resolves against the same definition, so a title the definition
    // no longer has must not reappear on the image.
    let out = html("[r]: /u\t\"T\"\n\n![t][r]\n");
    assert_no_title("reference image", &out);
    assert!(out.contains("<img"), "lost the image: {out}");
}

#[test]
fn a_unicode_space_does_not_pad_the_reference_definition_title_slot_either() {
    // This producer was a full Unicode `trim`, so it admitted the whole
    // White_Space property and not only the tab. Narrowing the terminal to a
    // literal `' '` drops both; narrowing it to `[' ', '\t']` would have
    // re-admitted the tab, and rejecting only a tab would have left these.
    assert_no_title("no-break space", &html("[r]: /u\u{a0}\"T\"\n\n[t][r]\n"));
    assert_no_title("em space", &html("[r]: /u\u{2003}\"T\"\n\n[t][r]\n"));
}

// --- reference definition, trailing attribute slot -------------------------

#[test]
fn a_tab_does_not_pad_the_reference_definition_attribute_slot() {
    for (label, src) in [
        ("tab first", "[r]: /u\t{.c}\n\n[t][r]\n"),
        ("space then tab", "[r]: /u \t{.c}\n\n[t][r]\n"),
        ("tab then space", "[r]: /u\t {.c}\n\n[t][r]\n"),
    ] {
        let out = html(src);
        assert!(!out.contains("class="), "{label}: took the block: {out}");
        assert!(
            out.contains("href=\"/u\""),
            "{label}: lost the definition: {out}"
        );
    }
}

// --- controls --------------------------------------------------------------

/// CONTROL. No mutation of any of the four slots breaks it - it states what the
/// fix must leave alone rather than what the fix changed, and it is not evidence
/// that any narrowing works.
#[test]
fn a_space_still_pads_every_one_of_the_four_slots() {
    assert!(
        html("[t](/u \"T\")\n").contains("title=\"T\""),
        "inline link"
    );
    assert!(
        html("![t](/u \"T\")\n").contains("title=\"T\""),
        "inline image"
    );
    assert!(
        html("[r]: /u \"T\"\n\n[t][r]\n").contains("title=\"T\""),
        "reference definition"
    );
    assert!(
        html("[r]: /u {.c}\n\n[t][r]\n").contains("class=\"c\""),
        "trailing attribute block"
    );
}

#[test]
fn every_slot_is_exactly_one_space() {
    // The production spells `link_title` as exactly one character while every
    // engine read a run. carve#912 answered which side gives: the productions
    // are right and the readers narrow. At all four slots a wider run means the
    // slot does not match - so the construct does not form and the characters
    // stay text, which is the failure PART 7 already names.
    assert!(
        !html("[t](/u  \"T\")\n").contains("title=\"T\""),
        "inline link"
    );
    assert!(
        !html("![t](/u  \"T\")\n").contains("title=\"T\""),
        "inline image"
    );
    assert!(
        !html("[r]: /u  \"T\"\n\n[t][r]\n").contains("title=\"T\""),
        "reference definition"
    );
    assert!(
        !html("[r]: /u  {.c}\n\n[t][r]\n").contains("class=\"c\""),
        "trailing attribute block"
    );
}

/// CONTROL. The `']' ':'` MARKER SEPARATOR on a definition line already rejected
/// a tab before this change, and the destination's own leading run is explicitly
/// whitespace by the production (`resources/grammar.ebnf`, the note at
/// `link_destination`). Both are watched so that narrowing the title slot is not
/// quietly extended along the line.
/// CONTROL, and the one that keeps this change from creeping along the line.
/// The run between a title and the closing `)` is NOT `link_title` - the
/// production puts no slot there at all, so neither a space nor a tab is
/// grammatical - and narrowing it is a separate question this ticket does not
/// answer. Watched, so that narrowing it later is a deliberate act rather than a
/// side effect.
#[test]
fn the_run_after_an_inline_title_is_left_alone() {
    assert!(
        html("[t](/u \"T\"\t)\n").contains("title=\"T\""),
        "a tab after the title stopped closing the link"
    );
    assert!(
        html("[t](/u \"T\" )\n").contains("title=\"T\""),
        "a space after the title stopped closing the link"
    );
}

#[test]
fn the_rest_of_the_definition_line_is_unchanged() {
    let out = html("[r]:\t/u\n\n[t][r]\n");
    assert!(
        !out.contains("<a "),
        "a tab separator defined a link: {out}"
    );

    let out = html("[r]: \t/u\n\n[t][r]\n");
    assert!(
        out.contains("href=\"/u\""),
        "the destination's leading run stopped being whitespace: {out}"
    );
}
