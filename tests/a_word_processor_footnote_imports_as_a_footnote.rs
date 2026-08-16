//! Footnote-shaped HTML from a word processor imports as real footnotes.
//!
//! The `word` and `google-docs` adapter names existed and dispatched nothing,
//! so every one of these documents imported as a literal link beside an
//! orphaned list: the reference kept its `#fn1` href and the note body became
//! an ordinary list item or paragraph.
//!
//! None of these producers uses the DPUB-ARIA roles a Carve engine writes.
//! What all of them share is a MUTUALLY LINKED ANCHOR PAIR - the body
//! reference points at the note and the note points back - and that pair, not
//! a vendor class name and not the `fn1`/`fnref1` id convention, is what binds
//! them here.
//!
//! The input shapes are verbatim excerpts of real exports, with the exports'
//! own line-wrapping tabs written as spaces:
//! - Word "Save as Web Page": bjanderson70/sf-cross-cutting-concerns,
//!   CCCDocs/home.htm
//! - Word "Save as Web Page, Filtered": cf-convention.github.io,
//!   Data/cf-documents/cf-governance/cf2_whitepaper_final.html
//! - Google Docs "Download as HTML": Flucille/Flucille,
//!   "Stalins political skills.html"
//! - LibreOffice 24.2 Writer HTML export, generated locally
//! - Pandoc 1.x: jgm/pandoc tests/writer.html at tag 1.19.2.4
//!
//! Ports markup-carve/carve-php#1303 and markup-carve/carve-js#1103.

use carve::{html_to_carve, to_html, HtmlImportAdapter, HtmlImportOptions};

/// Word writes `name=` rather than `id=` on both anchors, quotes three
/// attributes three different ways, and brackets the separator in a
/// downlevel-revealed conditional that is not a comment in the source.
const WORD_SAVE_AS_WEB_PAGE: &str = concat!(
    "<p class=MsoNormal>Static typing<a\n",
    "style='mso-footnote-id:ftn1' href=\"#_ftn1\" name=\"_ftnref1\" title=\"\"><span\n",
    "class=MsoFootnoteReference><span style='mso-special-character:footnote'>",
    "<![if !supportFootnotes]><span\n",
    "class=MsoFootnoteReference><span style='font-size:11.0pt'>[1]</span></span>",
    "<![endif]></span></span></a> matters.</p>\n",
    "<div style='mso-element:footnote-list'><![if !supportFootnotes]><br clear=all>\n",
    "<hr align=left size=1 width=\"33%\">\n",
    "<![endif]>\n",
    "<div style='mso-element:footnote' id=ftn1>\n",
    "<p class=MsoFootnoteText><a style='mso-footnote-id:ftn1' href=\"#_ftnref1\"\n",
    "name=\"_ftn1\" title=\"\"><span class=MsoFootnoteReference>",
    "<span style='mso-special-character:\n",
    "footnote'><![if !supportFootnotes]><span class=MsoFootnoteReference><span\n",
    "style='font-size:10.0pt'>[1]</span></span><![endif]></span></span></a>\n",
    "Static Object Orient Languages</p>\n",
    "</div>\n",
    "</div>",
);

/// The filtered save drops every `mso-element` style, so the wrapper is a bare
/// `<div id="ftn1">` and only the anchors still pair.
const WORD_FILTERED: &str = concat!(
    "<p>Data<a href=\"#_ftn1\" name=\"_ftnref1\" title=\"\">",
    "<span class=\"MsoFootnoteReference\">[1]</span></a> centre.</p>\n",
    "<div><br clear=\"all\">\n",
    "<hr align=\"left\" size=\"1\" width=\"33%\">\n",
    "<div id=\"ftn1\">\n",
    "<p class=\"MsoFootnoteText\"><a href=\"#_ftnref1\" name=\"_ftn1\" title=\"\">",
    "<span class=\"MsoFootnoteReference\">[1]</span></a>",
    " NCAS British Atmospheric Data Centre</p>\n",
    "</div>\n",
    "</div>",
);

/// Google Docs puts the `<sup>` OUTSIDE the anchor, gives every note its own
/// bare `<div>`, and leaves the separator as a body-level sibling.
const GOOGLE_DOCS: &str = concat!(
    "<p class=\"c4\"><span class=\"c7\">Stalin became General Secretary</span>",
    "<sup class=\"c1\"><a href=\"#ftnt1\" id=\"ftnt_ref1\">[1]</a></sup>",
    "<span class=\"c0\">&nbsp;in 1922</span>",
    "<sup class=\"c1\"><a href=\"#ftnt2\" id=\"ftnt_ref2\">[2]</a></sup>",
    "<span class=\"c0\">.</span></p><hr class=\"c10\">",
    "<div><p class=\"c5\"><a href=\"#ftnt_ref1\" id=\"ftnt1\">[1]</a>",
    "<span class=\"c2\">&nbsp;General Secretary of the Communist Party.</span></p></div>",
    "<div><p class=\"c5\"><a href=\"#ftnt_ref2\" id=\"ftnt2\">[2]</a>",
    "<span class=\"c2\">&nbsp;Roy Medvedev, Let History Judge, Page 3</span></p></div>",
);

/// LibreOffice names nothing `fn`: the pair is `sdfootnote1anc` against
/// `sdfootnote1sym`, and the id on the wrapper div is a third name again.
const LIBREOFFICE: &str = concat!(
    "<p>Body sentence one<a class=\"sdfootnoteanc\" name=\"sdfootnote1anc\"",
    " href=\"#sdfootnote1sym\"><sup>1</sup></a>\ncontinues.</p>\n",
    "<p>Second para<a class=\"sdfootnoteanc\" name=\"sdfootnote2anc\"",
    " href=\"#sdfootnote2sym\"><sup>2</sup></a>\nends.</p>\n",
    "<div id=\"sdfootnote1\"><p class=\"sdfootnote\"><a class=\"sdfootnotesym\"",
    " name=\"sdfootnote1sym\" href=\"#sdfootnote1anc\">1</a>The\n first note body.</p>\n</div>\n",
    "<div id=\"sdfootnote2\"><p class=\"sdfootnote\"><a class=\"sdfootnotesym\"",
    " name=\"sdfootnote2sym\" href=\"#sdfootnote2anc\">2</a>Note\n two para one.</p>\n",
    " <p class=\"sdfootnote\">Note two para two.</p>\n</div>",
);

/// Pandoc 1.x: `footnoteRef` in camelCase, no ARIA roles anywhere, and a
/// back-link carrying no attributes at all.
const PANDOC_1X: &str = concat!(
    "<p>Here is a footnote reference,<a href=\"#fn1\" class=\"footnoteRef\" id=\"fnref1\">",
    "<sup>1</sup></a> and another.</p>\n",
    "<div class=\"footnotes\">\n<hr />\n<ol>\n",
    "<li id=\"fn1\"><p>Here is the footnote.<a href=\"#fnref1\">&#8617;</a></p></li>\n",
    "</ol>\n</div>",
);

/// Every producer, with the note body text that has to survive the import.
const PRODUCERS: [(&str, &str, &str); 5] = [
    (
        "word save as web page",
        WORD_SAVE_AS_WEB_PAGE,
        "Static Object Orient Languages",
    ),
    (
        "word filtered",
        WORD_FILTERED,
        "NCAS British Atmospheric Data Centre",
    ),
    (
        "google docs",
        GOOGLE_DOCS,
        "General Secretary of the Communist Party.",
    ),
    ("libreoffice", LIBREOFFICE, "The first note body."),
    ("pandoc 1.x", PANDOC_1X, "Here is the footnote."),
];

fn imported_as(adapter: HtmlImportAdapter, html: &str) -> String {
    html_to_carve(
        html,
        &HtmlImportOptions {
            adapter,
            ..Default::default()
        },
    )
    .unwrap()
    .value
}

fn imported(html: &str) -> String {
    imported_as(HtmlImportAdapter::Word, html)
}

#[test]
fn the_note_becomes_a_definition_and_the_reference_binds_to_it() {
    for (name, html, body) in PRODUCERS {
        let out = imported(html);
        assert!(out.contains("[^1]"), "{name}: no reference in\n{out}");
        assert!(out.contains("[^1]: "), "{name}: no definition in\n{out}");
        assert!(out.contains(body), "{name}: body lost in\n{out}");
    }
}

/// A back-link is generated navigation, not content. Carried into the body it
/// renders as a stray link to a fragment that no longer exists, and the marker
/// it wraps (`[1]`, `1`, the return arrow) lands in the note's text.
#[test]
fn the_backlink_and_its_marker_do_not_reach_the_note_body() {
    for (name, html, _) in PRODUCERS {
        let out = imported(html);
        for stray in [
            "#_ftnref",
            "#fnref",
            "#ftnt_ref",
            "#sdfootnote1anc",
            "\u{21a9}",
            "[^1]: [1]",
            "[^1]: 1",
        ] {
            assert!(!out.contains(stray), "{name}: kept {stray} in\n{out}");
        }
    }
}

/// Every producer emits a rule between the body and the notes, and it is
/// chrome: Pandoc inside the section, Word inside the footnote-list div
/// bracketed by a downlevel conditional, Google Docs as a plain sibling.
#[test]
fn the_separator_does_not_import_as_a_thematic_break() {
    for (name, html, _) in PRODUCERS {
        let out = imported(html);
        for stray in ["---", "***", "supportFootnotes"] {
            assert!(!out.contains(stray), "{name}: kept {stray} in\n{out}");
        }
    }
}

#[test]
fn the_import_renders_back_as_a_footnote() {
    for (name, html, body) in PRODUCERS {
        let rendered = to_html(&imported(html));
        for wanted in [
            "role=\"doc-noteref\"",
            "role=\"doc-endnotes\"",
            "<li id=\"fn1\">",
            body,
        ] {
            assert!(
                rendered.contains(wanted),
                "{name}: no {wanted} in\n{rendered}"
            );
        }
    }
}

/// The adapter is the caller's declaration of provenance. `generic` takes
/// arbitrary HTML, where a mutually linked anchor pair is not proof of a
/// footnote, so it keeps reading only what a Carve engine writes.
#[test]
fn the_generic_adapter_leaves_the_shape_alone() {
    let out = imported_as(HtmlImportAdapter::Generic, LIBREOFFICE);

    assert!(!out.contains("[^1]"), "{out}");
    assert!(out.contains("#sdfootnote1sym"), "{out}");
}

/// The three editor adapters that are not word-processor exports stay out too.
#[test]
fn an_editor_adapter_that_is_not_a_word_processor_leaves_the_shape_alone() {
    for adapter in [
        HtmlImportAdapter::Tiptap,
        HtmlImportAdapter::Prosemirror,
        HtmlImportAdapter::Ckeditor,
        HtmlImportAdapter::Tinymce,
    ] {
        let out = imported_as(adapter, LIBREOFFICE);
        assert!(!out.contains("[^1]"), "{adapter:?}: {out}");
    }
}

#[test]
fn both_adapter_names_recognize_the_shape() {
    let word = imported_as(HtmlImportAdapter::Word, GOOGLE_DOCS);
    let docs = imported_as(HtmlImportAdapter::GoogleDocs, GOOGLE_DOCS);

    assert_eq!(word, docs);
    assert!(word.contains("[^1]"), "{word}");
}

/// A reference whose target does not exist is not a footnote: nothing binds it
/// and `[^1]` with no definition renders as the literal text `[^1]`, which
/// would lose the href as well. It stays the link the HTML spelled, so nothing
/// is lost and there is nothing to report.
#[test]
fn a_reference_with_no_target_stays_a_link() {
    let out = imported(
        "<p>Body<a href=\"#fn9\" class=\"footnote-ref\" id=\"fnref9\"><sup>9</sup></a> tail.</p>",
    );

    assert!(!out.contains("[^"), "{out}");
    assert!(out.contains("(#fn9)"), "{out}");
}

/// A definition nothing references stays ordinary content.
///
/// Importing it as a definition would be worse than it looks: Carve renders an
/// unreferenced definition as NOTHING, so text that was visible in the input
/// would silently vanish from the output while still sitting in the source. As
/// ordinary content it stays visible.
#[test]
fn an_unreferenced_definition_stays_visible_content() {
    let out = imported(concat!(
        "<p>Body<a href=\"#fn1\" class=\"footnote-ref\" id=\"fnref1\"><sup>1</sup></a> tail.</p>",
        "<section class=\"footnotes\"><hr /><ol>",
        "<li id=\"fn1\"><p>Note one.<a href=\"#fnref1\" class=\"footnote-back\">&#8617;</a></p></li>",
        "<li id=\"fn2\"><p>Nothing points here.</p></li>",
        "</ol></section>",
    ));
    let rendered = to_html(&out);

    assert!(out.contains("[^1]: Note one."), "{out}");
    assert!(!out.contains("[^2]"), "{out}");
    assert!(out.contains("Nothing points here."), "{out}");
    assert!(rendered.contains("Nothing points here."), "{rendered}");
}

/// Two references to one note both bind to it.
///
/// Only one of them can be the back-link's target, so the mutual pair that
/// confirms the note cannot confirm the second reference. It binds because it
/// addresses a block already known to be a note - which is why the unmarked
/// Google Docs spelling works as well as the marked Pandoc one.
#[test]
fn two_references_to_one_note_both_bind() {
    let shapes = [
        (
            "marked as footnote-ref",
            concat!(
                "<p>A<a href=\"#fn1\" class=\"footnote-ref\" id=\"fnref1\"><sup>1</sup></a>",
                " and B<a href=\"#fn1\" class=\"footnote-ref\" id=\"fnref1-2\"><sup>1</sup></a>.</p>",
                "<section class=\"footnotes\"><ol><li id=\"fn1\"><p>Shared.",
                "<a href=\"#fnref1\" class=\"footnote-back\">&#8617;</a></p></li></ol></section>",
            ),
        ),
        (
            "unmarked, google docs shaped",
            concat!(
                "<p>A<sup><a href=\"#ftnt1\" id=\"ftnt_ref1\">[1]</a></sup>",
                " and B<sup><a href=\"#ftnt1\" id=\"ftnt_ref1b\">[1]</a></sup>.</p>",
                "<div><p><a href=\"#ftnt_ref1\" id=\"ftnt1\">[1]</a> Shared.</p></div>",
            ),
        ),
    ];

    for (name, html) in shapes {
        let out = imported(html);
        assert_eq!(
            out.matches("[^1]: ").count(),
            1,
            "{name}: the note is defined once, in\n{out}"
        );
        assert_eq!(
            out.matches("[^1]").count() - out.matches("[^1]: ").count(),
            2,
            "{name}: both references must spell the note, in\n{out}"
        );
        assert!(out.contains("A[^1] and B[^1]."), "{name}: {out}");
    }
}

/// A note body is block content, not one line: the writer indents the
/// continuation so the paragraphs and the list stay inside the note.
///
/// The items come back as `<li>one</li>`: a bare-text `<li>` is a tight item
/// and the import keeps the source's tightness (corpus-convert 27). That is
/// what a plain `<ul><li>one</li></ul>` does under `generic` too - it is the
/// importer's standing shape, not something the note body does to it.
#[test]
fn a_note_body_keeps_its_blocks() {
    let rendered = to_html(&imported(concat!(
        "<p>A<a href=\"#fn1\" class=\"footnote-ref\" id=\"fnref1\"><sup>1</sup></a>.</p>",
        "<section class=\"footnotes\"><ol><li id=\"fn1\">",
        "<p>First para.</p><ul><li>one</li><li>two</li></ul>",
        "<p>Last para.<a href=\"#fnref1\" class=\"footnote-back\">&#8617;</a></p>",
        "</li></ol></section>",
    )));

    assert!(rendered.contains("<p>First para.</p>"), "{rendered}");
    assert!(rendered.contains("<li>one</li>"), "{rendered}");
    assert!(rendered.contains("<p>Last para."), "{rendered}");
    assert_eq!(rendered.matches("<li id=\"fn1\">").count(), 1, "{rendered}");
    let note = &rendered[rendered.find("<li id=\"fn1\">").unwrap()..];
    assert!(note.contains("</ul>"), "{rendered}");
}

/// Nothing here reads the `fn1`/`fnref1` convention. The pair is resolved
/// through the fragment each anchor addresses, and the label is assigned 1..N
/// over the notes in document order - `_ftn1` and `sdfootnote1sym` are
/// generated navigation an engine regenerates, and neither is a label any
/// Carve source could carry.
#[test]
fn ids_outside_the_convention_still_pair() {
    let out = imported(concat!(
        "<p>A<a href=\"#note-alpha\" name=\"mark-alpha\"><sup>*</sup></a>.</p>",
        "<div id=\"wrap-alpha\"><p><a name=\"note-alpha\" href=\"#mark-alpha\">*</a>",
        " Odd-id note.</p></div>",
    ));

    assert!(out.contains("A[^1]."), "{out}");
    assert!(out.contains("[^1]: Odd-id note."), "{out}");
    assert!(!out.contains("note-alpha"), "{out}");
}

/// The notes are numbered by definition order, so a document with several gets
/// one label each rather than all of them colliding on `1`.
#[test]
fn each_note_gets_its_own_label() {
    let out = imported_as(HtmlImportAdapter::GoogleDocs, GOOGLE_DOCS);

    for wanted in ["[^1]", "[^2]", "[^1]: ", "[^2]: "] {
        assert!(out.contains(wanted), "no {wanted} in\n{out}");
    }
}

/// The engine's own HTML already carries the roles, and naming an adapter must
/// not double-handle it.
#[test]
fn the_engines_own_html_still_imports_once_under_the_adapter() {
    let html = to_html("a[^n] b\n\n[^n]: the note body\n");

    let out = imported(&html);

    assert_eq!(out.matches("[^1]: ").count(), 1, "{out}");
    assert_eq!(
        to_html(&out),
        html,
        "importing the rendered HTML must reproduce it; imported source was:\n{out}"
    );
}
