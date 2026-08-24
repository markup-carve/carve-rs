//! The edges of the footnote pairing rule, one case per guard.
//!
//! These are the branches the producer fixtures never reach. Each one is a
//! decision that would otherwise sit in the code with nothing able to fail if
//! it were removed: how far a note's block may grow, which end of a mutual
//! pair is the reference, what a note's body may keep, and what the pass must
//! not touch.
//!
//! Ports markup-carve/carve-php#1303 and markup-carve/carve-php#1307.

use carve::{html_to_carve, to_html, HtmlImportAdapter, HtmlImportOptions};

/// One note with a marked back-link, for the cases whose subject is the
/// reference site rather than the note.
const NOTE: &str = concat!(
    "<section class=\"footnotes\"><ol><li id=\"fn1\"><p>The note.",
    "<a href=\"#fnref1\" class=\"footnote-back\">&#8617;</a></p></li></ol></section>",
);

fn import_as_word(html: &str) -> String {
    html_to_carve(
        html,
        &HtmlImportOptions {
            adapter: HtmlImportAdapter::Word,
            ..Default::default()
        },
    )
    .unwrap()
    .value
}

/// A note's block may not grow into the whole document.
///
/// Where the fragment lands on inline content with no block of its own, the
/// climb runs off the top - `<body>` and `<html>` are not definition blocks -
/// and taking what is left would move every paragraph into one note.
#[test]
fn a_note_block_that_would_be_the_whole_document_is_refused() {
    let out = import_as_word(concat!(
        "<span id=\"x\">loose target</span>",
        "<p>Body<a href=\"#x\" class=\"footnote-ref\" id=\"rx\"><sup>1</sup></a> tail.</p>",
    ));

    assert!(!out.contains("[^1]"), "{out}");
    assert!(out.contains("Body"), "{out}");
    assert!(out.contains("loose target"), "{out}");
}

/// The same refusal spelled as a full document, where the climb has the two
/// wrapper elements to run past explicitly.
#[test]
fn a_note_block_is_refused_when_the_climb_leaves_the_document() {
    let out = import_as_word(concat!(
        "<html><body><span id=\"x\">loose target</span>",
        "<p>Body<a href=\"#x\" class=\"footnote-ref\" id=\"rx\"><sup>1</sup></a> tail.</p>",
        "</body></html>",
    ));

    assert!(!out.contains("[^1]"), "{out}");
    assert!(out.contains("loose target"), "{out}");
}

/// The guarded climb counts targets addressed by `id` as well as by the legacy
/// `<a name>` the Word and LibreOffice fixtures use, so a wrapper holding one
/// `id`-addressed note is still the note's block.
#[test]
fn the_climb_counts_an_id_addressed_target() {
    let rendered = to_html(&import_as_word(concat!(
        "<p>Body<sup><a href=\"#ftnt1\" id=\"ftnt_ref1\">[1]</a></sup> tail.</p>",
        "<div id=\"wrap1\"><p><a href=\"#ftnt_ref1\" id=\"ftnt1\">[1]</a> First half.</p>",
        "<p>Second half.</p></div>",
    )));

    assert!(rendered.contains("<li id=\"fn1\">"), "{rendered}");
    assert!(rendered.contains("First half."), "{rendered}");
    let note = &rendered[rendered.find("<li id=\"fn1\">").unwrap()..];
    assert!(
        note.contains("Second half."),
        "the wrapper is the note, so its second paragraph stays inside the note:\n{rendered}"
    );
}

/// A reference carrying no id of its own has no pair to read from the other
/// end, and binds on its marker alone.
#[test]
fn a_reference_with_no_id_binds_on_its_marker() {
    let out = import_as_word(concat!(
        "<p>Body<a href=\"#fn1\" class=\"footnote-ref\"><sup>1</sup></a> tail.</p>",
        "<section class=\"footnotes\"><ol><li id=\"fn1\"><p>The note.</p></li></ol></section>",
    ));

    assert!(out.contains("Body[^1] tail."), "{out}");
    assert!(out.contains("[^1]: The note."), "{out}");
}

/// Where both ends of a mutual pair are addressable, the marked end is the
/// reference - document order does not get a say.
#[test]
fn a_marked_reference_wins_over_document_order() {
    let out = import_as_word(concat!(
        "<div id=\"note\"><p><a name=\"target\" href=\"#ref\">1</a> The note.</p></div>",
        "<p>Body<a href=\"#target\" name=\"ref\" class=\"footnote-ref\"><sup>1</sup></a> tail.</p>",
    ));

    assert!(out.contains("Body[^1] tail."), "{out}");
    assert!(out.contains("[^1]: The note."), "{out}");
}

/// Where neither end is marked as the reference but one is marked as the
/// back-link, the other end is the reference.
#[test]
fn a_back_link_marker_decides_the_other_end_is_the_reference() {
    let out = import_as_word(concat!(
        "<div id=\"note\"><p><a name=\"target\" href=\"#ref\" class=\"footnote-back\">1</a>",
        " The note.</p></div>",
        "<p>Body<a href=\"#target\" name=\"ref\"><sup>1</sup></a> tail.</p>",
    ));

    assert!(out.contains("Body[^1] tail."), "{out}");
    assert!(out.contains("[^1]: The note."), "{out}");
}

/// A block holding another note's block is a container, not a note. Keeping
/// both would move one subtree into two places at once.
#[test]
fn a_block_holding_another_note_is_not_itself_a_note() {
    let out = import_as_word(concat!(
        "<p>x<a href=\"#a\" name=\"ra\"><sup>1</sup></a> y<a href=\"#b\" name=\"rb\">",
        "<sup>2</sup></a></p>",
        "<div id=\"a\"><p>outer<a href=\"#ra\">back</a></p>",
        "<div id=\"b\"><p>inner<a href=\"#rb\">back</a></p></div></div>",
    ));

    assert_eq!(
        out.matches("]: ").count(),
        1,
        "only the inner block is a note, in\n{out}"
    );
    assert!(out.contains("[^1]: inner"), "{out}");
    assert!(out.contains("outer"), "{out}");
}

/// An ordinary link beside a note is left alone: only an anchor addressing a
/// note becomes a reference to it.
#[test]
fn an_external_link_is_not_swept_up() {
    let out = import_as_word(&format!(
        concat!(
            "<p>Body<a href=\"#fn1\" class=\"footnote-ref\" id=\"fnref1\"><sup>1</sup></a>",
            " and <a href=\"https://example.com\">a site</a>.</p>{}",
        ),
        NOTE
    ));

    assert!(out.contains("[a site](https://example.com)"), "{out}");
    assert!(out.contains("[^1]: The note."), "{out}");
}

/// A note's own body may address another note without that link turning into a
/// second reference to it.
#[test]
fn a_link_inside_a_note_is_not_a_reference() {
    let out = import_as_word(concat!(
        "<p>A<a href=\"#fn1\" class=\"footnote-ref\" id=\"fnref1\"><sup>1</sup></a>",
        " B<a href=\"#fn2\" class=\"footnote-ref\" id=\"fnref2\"><sup>2</sup></a>.</p>",
        "<section class=\"footnotes\"><ol>",
        "<li id=\"fn1\"><p>One, see <a href=\"#fn2\">the other</a>.",
        "<a href=\"#fnref1\" class=\"footnote-back\">&#8617;</a></p></li>",
        "<li id=\"fn2\"><p>Two.<a href=\"#fnref2\" class=\"footnote-back\">&#8617;</a></p></li>",
        "</ol></section>",
    ));

    assert!(out.contains("A[^1] B[^2]."), "{out}");
    assert!(out.contains("[the other](#fn2)"), "{out}");
    assert_eq!(out.matches("]: ").count(), 2, "{out}");
}

/// A genuine link in a note's body survives the back-link sweep.
#[test]
fn a_content_link_in_a_note_body_survives() {
    let out = import_as_word(concat!(
        "<p>Body<a href=\"#fn1\" class=\"footnote-ref\" id=\"fnref1\"><sup>1</sup></a>.</p>",
        "<section class=\"footnotes\"><ol><li id=\"fn1\">",
        "<p>See <a href=\"https://example.com/paper\">the paper</a>.",
        "<a href=\"#fnref1\" class=\"footnote-back\">&#8617;</a></p>",
        "</li></ol></section>",
    ));

    assert!(
        out.contains("[the paper](https://example.com/paper)"),
        "{out}"
    );
    assert!(!out.contains("#fnref1"), "{out}");
}

/// The wrapper a back-link sat in goes with it, rather than staying behind as
/// an empty superscript.
#[test]
fn the_wrapper_around_a_backlink_goes_with_it() {
    let out = import_as_word(concat!(
        "<p>Body<a href=\"#fn1\" class=\"footnote-ref\" id=\"fnref1\"><sup>1</sup></a>.</p>",
        "<section class=\"footnotes\"><ol><li id=\"fn1\"><p>The note.",
        "<sup><a href=\"#fnref1\" class=\"footnote-back\">&#8617;</a></sup></p></li></ol></section>",
    ));

    assert_eq!(out, "Body[^1].\n\n[^1]: The note.\n");
}

/// A comment sitting in the container the notes leave behind does not keep
/// that container alive.
#[test]
fn a_comment_does_not_keep_the_emptied_container_alive() {
    let out = import_as_word(concat!(
        "<p>Body<a href=\"#fn1\" class=\"footnote-ref\" id=\"fnref1\"><sup>1</sup></a>.</p>",
        "<div class=\"footnotes\"><!-- endnotes --><hr /><ol>",
        "<li id=\"fn1\"><p>The note.<a href=\"#fnref1\" class=\"footnote-back\">&#8617;</a></p></li>",
        "</ol></div>",
    ));

    assert_eq!(out, "Body[^1].\n\n[^1]: The note.\n");
}

/// A `<sup>` holding more than the reference is not the reference's site.
///
/// Only a superscript that wraps the anchor and nothing else is taken as the
/// site; one that also carries an element of its own keeps its content, and
/// the reference binds inside it.
#[test]
fn a_sup_holding_more_than_the_reference_survives() {
    let out = import_as_word(&format!(
        concat!(
            "<p>Body<sup><a href=\"#fn1\" class=\"footnote-ref\" id=\"fnref1\">1</a>",
            "<span>*</span></sup> t.</p>{}",
        ),
        NOTE
    ));
    let rendered = to_html(&out);

    assert!(out.contains("Body{^[^1]*^} t."), "{out}");
    assert!(rendered.contains("role=\"doc-noteref\""), "{rendered}");
    assert!(rendered.contains("*</sup>"), "{rendered}");
}

/// The same where the superscript's extra content is text rather than an
/// element - here brackets, which must not read as a wiki link on the way back
/// through the parser.
#[test]
fn a_sup_holding_text_beside_the_reference_survives() {
    let out = import_as_word(&format!(
        concat!(
            "<p>Body<sup>[<a href=\"#fn1\" class=\"footnote-ref\" id=\"fnref1\">1</a>]</sup>",
            " t.</p>{}",
        ),
        NOTE
    ));
    let rendered = to_html(&out);

    assert!(out.contains("Body{^[[^1]]^} t."), "{out}");
    assert!(
        rendered.contains("<sup>[<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\">"),
        "{rendered}"
    );
}

/// A separator written AFTER the notes is chrome too. The search only takes
/// what precedes the first note, so this one survives to pruning, which is
/// what stops it importing as a thematic break.
#[test]
fn a_separator_after_the_notes_does_not_survive() {
    let out = import_as_word(concat!(
        "<p>Body<a href=\"#fn1\" class=\"footnote-ref\" id=\"fnref1\"><sup>1</sup></a> t.</p>",
        "<div class=\"footnotes\"><ol><li id=\"fn1\"><p>The note.",
        "<a href=\"#fnref1\" class=\"footnote-back\">&#8617;</a></p></li></ol><hr /></div>",
    ));

    assert_eq!(out, "Body[^1] t.\n\n[^1]: The note.\n");
}

/// Notes written before the body still pair, and the separator search stops at
/// the top of the document rather than walking off it.
///
/// THE NOTES ARE NOT LAST HERE, so they keep the position they were written at:
/// the `::: footnotes` directive stands where the section stood, and the
/// definition hoists to the end like every other footnote definition
/// (markup-carve/carve#1627, carve-rs#1313). This assertion held the pre-ruling
/// output, where the section's position was discarded in silence - which is
/// what that ruling forbids. The subject of the test is unchanged: the pair
/// still binds, and the separator search still stops at the top of the document
/// instead of walking off it.
///
/// The placement is not gated on the `role="doc-endnotes"` spelling. The clause
/// is written over that spelling, but its argument - Carve HAS a way to say
/// where the section stood, so discarding it is a loss with nothing behind it -
/// does not depend on how the wrapper was marked, and a Word export's
/// `<section class="footnotes">` before the body renders in a different place
/// than one after it just the same. Measured against carve-js at `main`, which
/// writes this document byte for byte as it is written here and writes NO
/// directive for the same notes placed after the body.
#[test]
fn notes_written_before_the_body_still_pair() {
    let out = import_as_word(&format!(
        "<html><body>{}<p>Body<a href=\"#fn1\" class=\"footnote-ref\" id=\"fnref1\">\
         <sup>1</sup></a> tail.</p></body></html>",
        NOTE
    ));

    assert_eq!(
        out,
        "::: footnotes\n\n:::\n\nBody[^1] tail.\n\n[^1]: The note.\n"
    );
}

/// The same notes placed AFTER the body get no directive, which is what makes
/// the assertion above a statement about POSITION rather than a directive the
/// importer now always writes.
#[test]
fn notes_written_after_the_body_get_no_placement_directive() {
    let out = import_as_word(&format!(
        "<p>Body<a href=\"#fn1\" class=\"footnote-ref\" id=\"fnref1\">\
         <sup>1</sup></a> tail.</p>{}",
        NOTE
    ));

    assert_eq!(out, "Body[^1] tail.\n\n[^1]: The note.\n");
}

/// Word's downlevel-revealed conditionals arrive here as COMMENT nodes.
///
/// `<![if !supportFootnotes]>` is not a comment in the source, but html5ever
/// follows the HTML grammar and reads `<!` without `--` as a bogus comment.
/// carve-php reads the same bytes back as TEXT, because libxml has no such
/// production - so the two engines recognize the same chrome through different
/// node types, and this pins which one carve-rs is answering.
#[test]
fn a_downlevel_conditional_around_the_separator_is_chrome() {
    let out = import_as_word(concat!(
        "<p>Body<a href=\"#_ftn1\" name=\"_ftnref1\">[1]</a> t.</p>",
        "<div><![if !supportFootnotes]><br clear=all>\n<hr>\n<![endif]>\n",
        "<div id=ftn1><p><a href=\"#_ftnref1\" name=\"_ftn1\">[1]</a> The note.</p></div></div>",
    ));

    assert_eq!(out, "Body[^1] t.\n\n[^1]: The note.\n");
}

/// The note's marker anchor goes even when it does not point at the reference.
///
/// Word, Google Docs and LibreOffice each write the note's fragment target,
/// its back-link and its visible marker (`[1]`, `1`, the return arrow) as ONE
/// anchor, and the sweep has a clause for exactly that: an anchor that IS a
/// target the reference addressed, carrying a fragment href. In every producer
/// fixture that anchor also points back at the reference, so the clause is
/// invisible there - what reaches it is a reference with no id of its own,
/// which leaves nothing for the back-link test to match against.
///
/// Without the clause the marker `1` and a link to a fragment that no longer
/// exists are written into the note's own text.
#[test]
fn a_marker_anchor_that_points_at_no_bound_reference_still_goes() {
    let out = import_as_word(concat!(
        "<p>Body<a class=\"footnote-ref\" href=\"#sdfootnote1sym\">1</a> tail.</p>",
        "<div id=\"sdfootnote1\"><p><a name=\"sdfootnote1sym\" href=\"#sdfootnote1anc\">1</a>",
        "The note.</p></div>",
    ));

    assert_eq!(out, "Body[^1] tail.\n\n[^1]: The note.\n");
}
