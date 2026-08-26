//! A definition body's separator is a RUN of spaces, and its width is the
//! body's content column.
//!
//! markup-carve/carve#1757: the body marker required `:` plus exactly TWO
//! spaces, making it the only marker in the language that would not take a
//! single separator space - `- item`, `1. item`, `> quote` and `:: term` all
//! do. Two predicates in the definition-list loop already disagreed about it:
//! entry detection accepted one space, so a one-space line BROKE a term's fold,
//! and the body matcher then refused to collect it, leaving the line as a stray
//! paragraph.
//!
//! The separator is now a run of one or more spaces and the body's content
//! column is `1 + separator width`, so `: x` opens column 2 and `:  x` column
//! 3. Each spelling's continuation qualifies by reaching ITS OWN body's column,
//! which is the rule PART 9 §24 C1 already applies to footnote bodies and list
//! items. Both spellings may appear in one list.
//!
//! ONE SPACE IS ALSO CANONICAL, so the writer narrows the separator - and
//! carries the body's continuations down by the same amount, because narrowing
//! the separator narrows the column they have to reach. A writer that trimmed
//! one and not the other would change what the document says.
//!
//! The four `.fmt` sidecars markup-carve/carve#1757 rewrote still spell `:  `
//! at the pin this repo is on, so the writer half is declared AHEAD OF THE PIN
//! in `corpus_canonical_form.rs` rather than blocked by it. That declaration
//! retires itself: each entry asserts the pinned sidecar still DISAGREES, so it
//! starts failing the moment the pin moves past it.
//!
//! THE TWO-SPACE CONTROL IS THE POINT OF THE SET. A change that made every
//! separator behave like one space would pass every headline case here and
//! still be wrong, because the column would be hard-coded to the new value
//! instead of derived from the width. The control asserts the OLD answer for
//! the OLD spelling, and it is the only thing that can tell the two apart.

use carve::{to_carve, to_html};

fn html(source: &str) -> String {
    to_html(source).trim().to_string()
}

// ---------------------------------------------------------------------------
// The parser: the width sets the column
// ---------------------------------------------------------------------------

#[test]
fn one_space_opens_a_body() {
    assert_eq!(
        html(":: term\n: definition\n"),
        "<dl>\n  <dt>term</dt>\n  <dd>definition</dd>\n</dl>"
    );
}

#[test]
fn a_one_space_body_takes_a_continuation_at_column_two() {
    assert_eq!(
        html(":: term\n: first\n\n  second\n"),
        "<dl>\n  <dt>term</dt>\n  <dd>\n    <p>first</p>\n    <p>second</p>\n  </dd>\n</dl>"
    );
}

#[test]
fn column_one_does_not_reach_a_one_space_body() {
    // The other side of that boundary: one column short, the body ends and the
    // line is the document's own paragraph.
    assert_eq!(
        html(":: term\n: first\n\n second\n"),
        "<dl>\n  <dt>term</dt>\n  <dd>first</dd>\n</dl>\n<p>second</p>"
    );
}

#[test]
fn the_two_space_control_keeps_column_three() {
    // THE CONTROL. Same continuation column as the one-space case above, which
    // folds in there and must NOT fold in here - the column came from the
    // separator's width, not from a new constant.
    assert_eq!(
        html(":: term\n:  first\n\n  second\n"),
        "<dl>\n  <dt>term</dt>\n  <dd>first</dd>\n</dl>\n<p>second</p>"
    );
}

#[test]
fn both_spellings_may_appear_in_one_list() {
    assert_eq!(
        html(":: term\n: one\n:  two\n"),
        "<dl>\n  <dt>term</dt>\n  <dd>one</dd>\n  <dd>two</dd>\n</dl>"
    );
}

#[test]
fn the_first_block_form_works_on_the_narrow_width() {
    assert_eq!(
        html(":: term\n: +\nflush block\n"),
        "<dl>\n  <dt>term</dt>\n  <dd>flush block</dd>\n</dl>"
    );
}

#[test]
fn a_colon_line_below_a_folding_term_is_the_body() {
    // The line the two predicates used to disagree about: the term folds its
    // wrapped line, and the one-space colon line then breaks the fold and opens
    // the body rather than falling out of the loop as a stray paragraph.
    assert_eq!(
        html(":: term\nwrapped on\n: definition\n"),
        "<dl>\n  <dt>term\nwrapped on</dt>\n  <dd>definition</dd>\n</dl>"
    );
}

#[test]
fn a_wider_run_is_its_own_column() {
    // The rule is the separator's WIDTH, with no ceiling on it - the same rule
    // a bullet already follows, where `-   first` puts its content column at 4.
    // No corpus document uses a run wider than two, so nothing else in this
    // repository pins the general form. Measured: capping the content column at
    // 3 - identity at width 1 and 2, wrong above them - fails ONE test in the
    // whole suite, and it is this one.
    for separator in 1..=6 {
        let marker = " ".repeat(separator);
        let column = 1 + separator;
        let reaches = html(&format!(
            ":: term\n:{marker}first\n\n{}second\n",
            " ".repeat(column)
        ));
        assert_eq!(
            reaches,
            "<dl>\n  <dt>term</dt>\n  <dd>\n    <p>first</p>\n    <p>second</p>\n  </dd>\n</dl>",
            "separator {separator}: a continuation AT the body's column"
        );

        let falls_short = html(&format!(
            ":: term\n:{marker}first\n\n{}second\n",
            " ".repeat(column - 1)
        ));
        assert_eq!(
            falls_short, "<dl>\n  <dt>term</dt>\n  <dd>first</dd>\n</dl>\n<p>second</p>",
            "separator {separator}: one column short of it"
        );
    }
}

#[test]
fn a_tab_is_not_a_separator() {
    // carve-rs#518 ruled the separator a SPACE, not a tab, and that stands: a
    // tab after the colon is content, so the marker opens no body and the line
    // folds into the term above it.
    let output = html(":: term\n:\tdefinition\n");
    assert!(!output.contains("<dd>"), "{output}");
}

#[test]
fn a_marker_with_only_a_separator_opens_nothing() {
    // An empty remainder is the placeholder form, not a body with content -
    // true at either width.
    for marker in [":", ":  "] {
        let output = html(&format!(":: term\n{marker}\n"));
        assert!(!output.contains("<dd>"), "{marker:?}: {output}");
    }
}

#[test]
fn the_term_marker_and_the_colon_fence_are_untouched() {
    // Both need a SINGLE colon before the separator, so neither can match the
    // body marker. `::` is the term and `:::` opens a fence.
    assert!(html(":: term\n: d\n").contains("<dt>term</dt>"));
    assert!(html("::: note\ntext\n:::\n").contains("<aside class=\"admonition note\""));
}

#[test]
fn a_content_less_marker_is_content_less_at_every_width() {
    // THE SIDE EFFECT OF A GREEDY RUN, decided rather than inherited. The run
    // takes every space, so a line that is only a marker has no content at any
    // width - it is the placeholder form, not a body holding spaces. A
    // non-greedy separator would leave the leftovers as content and open an
    // empty `<dd>` at some widths and not others, which is a difference no
    // author could see and nothing pinned.
    for separator in 1..=5 {
        let output = html(&format!(":: term\n:{}\n", " ".repeat(separator)));
        assert!(!output.contains("<dd>"), "separator {separator}: {output}");
    }
}

#[test]
fn the_content_column_reaches_the_prepass_too() {
    // THE COLUMN IS READ IN MORE THAN ONE PLACE, and the collector is only the
    // most obvious. A comment fence is scoped by the column it REACHES, and
    // that column comes from the prepass's content-column stack - so a fence
    // written at a one-space body's column 2 is inside the body only if the
    // prepass registered 2 rather than a constant 3. Before this rule the whole
    // construct was term text.
    assert_eq!(
        html(":: term\n: body\n  %%%\n  hidden\n  %%%\n\nafter\n"),
        "<dl>\n  <dt>term</dt>\n  <dd>body</dd>\n</dl>\n<p>after</p>"
    );
}

// ---------------------------------------------------------------------------
// The writer: one space is canonical, and the body moves with it
// ---------------------------------------------------------------------------

#[test]
fn the_writer_emits_one_space() {
    assert_eq!(
        to_carve(":: term\n:  definition\n"),
        ":: term\n: definition\n"
    );
}

#[test]
fn narrowing_the_separator_carries_the_body_with_it() {
    // The half a writer can drop silently. The body holds a fenced block at the
    // two-space column; trimming the separator without re-indenting the fence
    // would leave it one column past a body whose column just shrank.
    let source = ":: t\n:  d\n\n   ```\n   a\n\n   b\n   ```\n";
    let written = to_carve(source);
    assert_eq!(written, ":: t\n: d\n\n  ```\n  a\n\n  b\n  ```\n");
    assert_eq!(html(&written), html(source));
}

#[test]
fn a_hoisted_definition_written_back_takes_the_narrow_separator_too() {
    // The writer has TWO branches that emit this separator: the ordinary body,
    // and the one that writes a collected link or footnote definition back onto
    // the description line it was authored on. They move together, or the same
    // document canonicalizes two ways depending on what its body held.
    assert_eq!(
        to_carve(":: term\n:  [r]: /u\n\nsee [t][r]\n"),
        ":: term\n: [r]: /u\n\nsee [t][r]\n"
    );
}

#[test]
fn the_canonical_form_is_a_fixed_point_that_preserves_the_document() {
    for source in [
        ":: term\n: definition\n",
        ":: term\n:  definition\n",
        ":: term\n:    definition\n\n     wide\n",
        ":: t\n:  d\n\n   ```\n   a\n   ```\n",
        ":: term\n: one\n:  two\n",
        ":: term\n: +\nflush block\n",
    ] {
        let once = to_carve(source);
        assert_eq!(to_carve(&once), once, "{source:?}");
        assert_eq!(html(&once), html(source), "{source:?}");
    }
}
