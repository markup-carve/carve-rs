//! A container `[label]` is a leaf inline host, so `CARVE-P9-041`'s trailing
//! comment rule reaches it (`markup-carve/carve#2552`, merged as carve#2562).
//!
//! The label's run begins mid-line, so nothing at the block layer covers a `%%`
//! written in it. Corpus category 518 pins the two spellings the clause
//! distinguishes, and these are those rows plus the hosts the corpus does not
//! reach.

use carve::to_html;

fn label_of(html: &str) -> String {
    let at = html
        .find("<p class=\"div-label\">")
        .unwrap_or_else(|| panic!("no div label in {html}"));
    let rest = &html[at + "<p class=\"div-label\">".len()..];
    rest[..rest.find("</p>").expect("unclosed div label")].to_string()
}

/// Corpus 518-9: a tab separates the marker as a space does.
#[test]
fn a_tab_separates_a_labels_trailing_comment() {
    assert_eq!(
        label_of(&to_html("::: note [a\t%% hidden]\nbody\n:::\n")),
        "a"
    );
}

/// Corpus 518-10: a marker that starts the label's run needs no separator, and
/// the empty label still publishes its paragraph.
#[test]
fn a_marker_starting_a_labels_run_needs_no_separator() {
    assert_eq!(label_of(&to_html("::: note [%% hidden]\nbody\n:::\n")), "");
}

/// The whole separating run goes with the comment, not one character of it.
#[test]
fn a_labels_whole_separating_run_goes_with_the_comment() {
    assert_eq!(label_of(&to_html(":::[a  %% hidden]\nbody\n:::\n")), "a");
    assert_eq!(label_of(&to_html(":::[a \t %% hidden]\nbody\n:::\n")), "a");
}

/// No separator, no comment. This is the guard that keeps a percent literal and
/// an ordinary `50%%` out of the rule.
#[test]
fn a_label_without_a_separator_keeps_its_percent_signs() {
    assert_eq!(
        label_of(&to_html(":::[a%%b and 50%%]\nbody\n:::\n")),
        "a%%b and 50%%"
    );
}

/// A backtick run is opaque to the marker (`carve#2547`), so the rule must not
/// delete text an author wrote inside one. An UNCLOSED run reaches the end of
/// the label, which is where the oracle's own inline run for a label ends.
#[test]
fn a_labels_backtick_run_is_opaque_to_the_marker() {
    assert_eq!(label_of(&to_html(":::[`%% x`]\nbody\n:::\n")), "`%% x`");
    assert_eq!(
        label_of(&to_html(":::[a ` b %% c]\nbody\n:::\n")),
        "a ` b %% c"
    );
}

/// Every host that surfaces an unconsumed label reads the same rule: the core
/// admonition and div, the extension floor, and the two built-ins that render
/// the label themselves rather than letting the floor do it.
#[test]
fn every_label_host_reads_the_same_rule() {
    for source in [
        "::: note [a\t%% hidden]\nbody\n:::\n",
        ":::[a\t%% hidden]\nbody\n:::\n",
        "::: spoiler \"T\" [a\t%% hidden]\nbody\n:::\n",
        "::: list-table \"T\" [a\t%% hidden]\n- - x\n  - y\n:::\n",
        "::: footnotes [a\t%% hidden]\n:::\n\nx[^1]\n\n[^1]: n\n",
    ] {
        assert_eq!(label_of(&to_html(source)), "a", "source: {source:?}");
    }
}

/// THE RULE HAS TWO SPELLINGS and this is what holds them together.
///
/// `parse_inlines` reads the trailing comment inside its byte loop for every
/// other host; `label_without_trailing_comment` reads it again for a label,
/// because a label is authored text rather than a parsed inline run. A shape
/// that answers differently in the two is drift, so each of these is measured
/// once as a paragraph and once as a label.
#[test]
fn a_label_and_an_inline_run_read_the_same_comment() {
    for (text, kept) in [
        ("a\t%% hidden", "a"),
        ("a %% hidden", "a"),
        ("a  %% hidden", "a"),
        ("%% hidden", ""),
        ("a%%b and 50%%", "a%%b and 50%%"),
        ("a %%", "a"),
        ("plain", "plain"),
    ] {
        let paragraph = to_html(&format!("{text}\n"));
        let expected = if kept.is_empty() {
            String::new()
        } else {
            format!("<p>{kept}</p>")
        };
        assert_eq!(paragraph, expected, "paragraph: {text:?}");
        assert_eq!(
            label_of(&to_html(&format!(":::[{text}]\nbody\n:::\n"))),
            kept,
            "label: {text:?}"
        );
    }
}

/// A `%%` SCOPED INSIDE A CLOSED INLINE CONSTRUCT IS NOT A TRAILING COMMENT and
/// the label keeps it.
///
/// The clause's marker consumes every character up to the line break. In
/// `{+a %% secret+}` the construct's closer follows the marker, so the parser
/// scopes the comment inside the insertion and it never reaches the line break.
/// A label is authored text, so the only two answers available are the text as
/// written and a cut at the marker - and the cut would delete `secret+}` and
/// leave a construct with no closer. Publishing the text is the answer that
/// loses nothing.
///
/// A paragraph hides the comment because it RENDERS the insertion's children,
/// which a label cannot do while it is text rather than an inline run. That gap
/// is `markup-carve/carve#2572`, and closing it removes this case rather than
/// changing the answer to it.
#[test]
fn a_marker_inside_a_closed_construct_is_not_trailing() {
    assert_eq!(
        label_of(&to_html(":::[{+a %% secret+}]\nbody\n:::\n")),
        "{+a %% secret+}"
    );
    // A marker AFTER the closer is trailing, and does cut.
    assert_eq!(label_of(&to_html(":::[{+a+} %% t]\nbody\n:::\n")), "{+a+}");
}

/// THE ENUMERATION IS MEASURED, NOT ASSUMED.
///
/// `label_without_trailing_comment` has to skip every construct that can hold a
/// `%%` without it being a marker, and one missed there DELETES visible text. So
/// the expected boundary is derived from the rule itself rather than written out:
/// the same text is put in a DEFINITION TERM, whose inline run the parser reads
/// through `parse_inlines`, positions are turned on, the trailing comment node's
/// source offset is read off it, and the label has to cut exactly there.
///
/// A term is the reference host because its run begins mid-line, as a label's
/// does, and corpus 518-3 and 518-4 pin both of the clause's spellings there.
#[test]
fn a_label_cuts_where_the_inline_rule_says_it_cuts() {
    use carve::{InlineNode, Options};

    const TERM_PREFIX: usize = 3; // ":: "

    fn comment_offset(text: &str) -> Option<usize> {
        let source = format!(":: {text}\n: d\n");
        let doc = carve::parse_with_options(
            &source,
            &Options {
                positions: true,
                ..Options::default()
            },
        );
        let carve::BlockNode::DefinitionList(list) = doc.children.first().expect("a block") else {
            panic!("not a definition list: {text:?}");
        };
        let term = &list
            .items
            .first()
            .expect("an item")
            .terms
            .first()
            .expect("a term")
            .children;
        term.iter().find_map(|node| match node {
            InlineNode::Comment(c) if !c.delimited => {
                Some(c.pos.as_ref().expect("positions on").start_offset - TERM_PREFIX)
            }
            _ => None,
        })
    }

    for text in [
        "a %% c",
        "a\t%% c",
        "a  %% c",
        "a \t %% c",
        "%% c",
        "a%%b",
        "a%%b and 50%%",
        "a %%",
        "plain text",
        "`%% x`",
        "a `%% x` b",
        "a ` b %% c",
        "``%% x`` %% c",
        "a `x` %% c",
        "{% %% hidden %} after",
        "before {% c %} %% tail",
        "{% a %}%% t",
        "{%a {%b {%c %% t",
        "{# %% hidden #} after",
        "{+a %% secret+}",
        "{-a %% secret-}",
        "{+a+} %% t",
        "before {# c #} %% tail",
        "a \\` %% hidden",
        "a \\{% x %} %% c",
        "a \\%% c",
        "\\%% c",
        "a `x\\` %% c",
        // A `]` cannot appear in a label at all - it closes the slot - so a link
        // and a bracketed span have no label spelling to compare.
        "*b* %% c",
        "x{.cls} %% c",
    ] {
        let expected = match comment_offset(text) {
            Some(at) => text[..at].trim_end_matches([' ', '\t']),
            None => text,
        };
        let escaped = expected
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        assert_eq!(
            carve::to_html(&format!(":::[{text}]\nbody\n:::\n")),
            format!("<div>\n  <p class=\"div-label\">{escaped}</p>\n  <p>body</p>\n</div>"),
            "text: {text:?} expected label {expected:?}"
        );
    }
}
