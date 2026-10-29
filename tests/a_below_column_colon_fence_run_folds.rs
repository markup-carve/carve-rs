//! A COLON-FENCE RUN BELOW EVERY LIVE CONTENT COLUMN FOLDS WHOLE
//! (markup-carve/carve-rs#1510).
//!
//! `- - - x` opens items at columns 2, 4 and 6. A `:::` run written at column 1
//! reaches none of them, so the executable spec (`tests/spec` at carve
//! `86569bd`) folds all four lines into the open paragraph as lazy text. This
//! engine folded the opener and the body and let the CLOSER out, publishing a
//! `<div>` with no content and no attributes one level up.
//!
//! THE COLUMN IS WHAT MAKES THE LINE TEXT. `dedent_for_collection` spends
//! exactly one column on a block-shaped below-column line for that reason;
//! `collect_trailing_lazy_through` took every lazy line flush, so the same
//! `:::` was spelled indented in one chunk and flush in the next, and the
//! re-parse of the flush spelling read a container closer. The two collectors
//! now answer with one rule (`dedent_below_column`).
//!
//! MEASURED, NOT ASSUMED. The carve-rs#1509 sweep, re-derived: 306
//! prefix/column pairs over the l(ist)/q(uote) container prefixes to depth
//! four, the line written at every column from just past the last quote marker
//! out to 14, sixteen line kinds each - 4896 documents - rendered through
//! `carve::to_html` and through the executable spec at the pinned corpus.
//! Before: the div kind disagreed on 3 pairs and every other kind on 0. After:
//! 0 everywhere, with exactly those 3 documents changing answer and nothing
//! else moving.

use carve::{to_html, to_html_with_options, Options};

/// The layout facade behind `to_html` declines any source containing `:::`, so
/// every document here reaches the authoritative pipeline either way - but the
/// paired render is kept as the standing guard carve-rs#1511 added: a parse
/// that answers differently once positions are on is a defect on its own.
fn both_paths(src: &str) -> String {
    let facade = to_html(src);
    let authoritative = to_html_with_options(src, &Options::default().with_positions(true));
    assert_eq!(
        facade, authoritative,
        "the library path and the position-tracking path disagree on {src:?}"
    );
    facade
}

#[test]
fn the_reported_document_folds_the_whole_run() {
    assert_eq!(
        both_paths("- - - x\n ::: note\n b\n :::\n").trim(),
        concat!(
            "<ul>\n",
            "  <li>\n",
            "    <ul>\n",
            "      <li>\n",
            "        <ul>\n",
            "          <li>x</li>\n",
            "        </ul>\n",
            "      </li>\n",
            "    </ul>\n",
            "  </li>\n",
            "</ul>\n",
            "<aside class=\"admonition note\" aria-label=\"Note\">\n",
            "  <p>b</p>\n",
            "</aside>",
        ),
    );
}

#[test]
fn every_pair_the_sweep_found_folds() {
    // The three (prefix, column) pairs, in the sweep's notation: `lll` col 1,
    // `llll` col 1, `qlll` col 3 - each a column below the outermost item's own
    // content column.
    for src in [
        "- - - x\n ::: note\n b\n :::\n",
        "- - - - x\n ::: note\n b\n :::\n",
        "> - - - x\n>  ::: note\n>  b\n>  :::\n",
    ] {
        let html = both_paths(src);
        assert!(
            html.contains("<li>x</li>"),
            "the run folded into the item: {src:?}: {html}"
        );
        assert!(
            html.contains("<aside class=\"admonition note\" aria-label=\"Note\">\n")
                && html.contains("<p>b</p>"),
            "the run did not open a note: {src:?}: {html}"
        );
    }
}

#[test]
fn the_closer_alone_was_never_the_whole_story() {
    // WITHOUT the plain `b` between them the document was already right, which
    // is what made the defect look like a colon-fence question and hid that it
    // was a collection question: the lazy collector only reached this run once
    // an ordinary lazy line had ended the indented collection above it.
    for (src, item, block) in [
        ("- - - x\n :::\n", "<li>x</li>", "<div>\n\n</div>"),
        (
            "- - - x\n ::: note\n :::\n",
            "<li>x</li>",
            "<aside class=\"admonition note\" aria-label=\"Note\">\n\n</aside>",
        ),
        ("- - - x\n b\n :::\n", "<li>x\nb</li>", "<div>\n\n</div>"),
    ] {
        let html = both_paths(src);
        assert!(html.contains(item), "{src:?}: {html}");
        assert!(html.ends_with(block), "{src:?}: {html}");
        assert!(!html.contains(":::"), "{src:?}: {html}");
    }
}

#[test]
fn a_run_at_a_live_content_column_still_opens() {
    // THE CONTROL. One column further in the run REACHES the outermost item and
    // opens there - what carve-rs#1509 ruled - so a fix that folded every
    // indented colon fence would take this with it.
    let html = both_paths("- - - x\n  ::: note\n  b\n  :::\n");
    assert!(
        html.contains("<aside class=\"admonition note\" aria-label=\"Note\">"),
        "{html}"
    );
    assert!(html.contains("<li>x</li>"), "{html}");
}

#[test]
fn a_shallower_ladder_was_right_all_along() {
    // Two levels deep the same document already folded: the divergence needed a
    // third level, because it takes one re-parse to strip the column and a
    // second to read what is left as a closer.
    let html = both_paths("- - x\n ::: note\n b\n :::\n");
    assert!(html.contains("<li>x</li>"), "{html}");
    assert!(
        html.ends_with(
            "<aside class=\"admonition note\" aria-label=\"Note\">\n  <p>b</p>\n</aside>"
        ),
        "{html}"
    );
}

#[test]
fn other_below_column_openers_still_fold_too() {
    // The rule is about the COLUMN, not about colons: every block-shaped line
    // in this band is lazy text, and the sweep puts all sixteen line kinds at 0
    // divergences. Each expectation is the executable spec's output.
    for (src, opened) in [
        (
            "- - - x\n # H\n b\n # H2\n",
            "<section id=\"H\">\n  <h1>H</h1>\n  <p>b</p>\n</section>\n<section id=\"H2\">\n  <h1>H2</h1>\n</section>",
        ),
        ("- - - x\n ---\n b\n", "<hr>\n<p>b</p>"),
        (
            "- - - x\n | A |\n b\n",
            "<table>\n  <tbody>\n    <tr><td>A</td></tr>\n  </tbody>\n</table>\n<p>b</p>",
        ),
    ] {
        let html = both_paths(src);
        assert!(html.contains("<li>x</li>"), "{src:?}: {html}");
        assert!(
            html.ends_with(opened),
            "a below-column line did not open its block: {src:?}: {html}"
        );
    }
}

#[test]
fn the_two_exceptions_still_dedent_all_the_way() {
    // THE OTHER CONTROL, and the reason this is not simply "keep every
    // column". A definition body (`:  `) attaches to the term above it from ANY
    // column, and a comment renders nothing at any column - the two exceptions
    // `dedent_below_column` carries with it, and a rule that kept every
    // block-shaped line's column would take both.
    let definition = both_paths("- one\n  :: term\n   :  def\n");
    assert!(
        definition.contains("<dt>term</dt>\n      <dd>def</dd>"),
        "{definition}"
    );

    // The comment is invisible and the line below it stays in the item, which
    // is the executable spec's own answer for this document.
    let comment = both_paths("- - - a\n b\n %% c\n d\n");
    assert!(comment.contains("<li>a\nb</li>"), "{comment}");
    assert!(!comment.contains("%%"), "{comment}");
    assert!(comment.ends_with("</ul>\n<p>d</p>"), "{comment}");
}
