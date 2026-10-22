//! Continuation marker in a block quote (Carve, grammar PART 9 §17).
//!
//! A lone `+` at column 0 immediately after a quoted line attaches the
//! following flush-left block to the quote body, with no `>` prefix and no
//! blank line -- the un-prefixed analogue of the list-item continuation marker.
//! It only attaches; a blank line still ends the quote, and a `+` outside any
//! container stays literal text. Output matches carve-php and carve-js.

#[test]
fn attaches_list_to_quote() {
    assert_eq!(
        carve::to_html("> quoted\n+\n- item"),
        "<blockquote>\n  <p>quoted</p>\n  <ul>\n    <li>item</li>\n  </ul>\n</blockquote>"
    );
}

#[test]
fn glued_quote_markers_are_paragraph_text() {
    for source in [">+", ">>+", ">> +"] {
        assert_eq!(
            carve::to_html(source),
            format!("<p>{}</p>", source.replace('>', "&gt;"))
        );
    }
    assert_eq!(
        carve::to_html("> >+"),
        "<blockquote><p>&gt;+</p></blockquote>"
    );
}

#[test]
fn quote_level_marker_attaches_a_quote_and_survives_a_blank_quote_line() {
    for (source, expected) in [
        ("> > q\n> +\n> > r", "<blockquote>\n  <blockquote>\n    <p>q</p>\n    <blockquote><p>r</p></blockquote>\n  </blockquote>\n</blockquote>"),
        ("> - a\n> +\n> > r", "<blockquote>\n  <ul>\n    <li>a\n      <blockquote><p>r</p></blockquote>\n    </li>\n  </ul>\n</blockquote>"),
        ("> > - a\n> +\n> para", "<blockquote>\n  <blockquote>\n    <ul>\n      <li>a</li>\n    </ul>\n    <p>para</p>\n  </blockquote>\n</blockquote>"),
        ("> - a\n>\n> +\n> para", "<blockquote>\n  <ul>\n    <li>a\n      para\n    </li>\n  </ul>\n</blockquote>"),
        ("> a\n> +\n> > q", "<blockquote>\n  <p>a\n+</p>\n  <blockquote><p>q</p></blockquote>\n</blockquote>"),
        ("> > q\n> > +\n> > - s", "<blockquote>\n  <blockquote><p>q\n+\n- s</p></blockquote>\n</blockquote>"),
    ] {
        assert_eq!(carve::to_html(source), expected, "{source:?}");
    }
}

#[test]
fn attaches_fenced_code_to_quote() {
    assert_eq!(
        carve::to_html("> quoted\n+\n```\ncode\n```"),
        "<blockquote>\n  <p>quoted</p>\n  <pre><code>code\n</code></pre>\n</blockquote>"
    );
}

#[test]
fn attaches_table_to_quote() {
    assert_eq!(
        carve::to_html("> quoted\n+\n| a | b |"),
        "<blockquote>\n  <p>quoted</p>\n  <table>\n    <tbody>\n      <tr><td>a</td><td>b</td></tr>\n    </tbody>\n  </table>\n</blockquote>"
    );
}

#[test]
fn two_markers_attach_two_blocks() {
    assert_eq!(
        carve::to_html("> q\n+\n- a\n+\n```\nc\n```"),
        "<blockquote>\n  <p>q</p>\n  <ul>\n    <li>a</li>\n  </ul>\n  <pre><code>c\n</code></pre>\n</blockquote>"
    );
}

#[test]
fn quote_resumes_after_attached_block() {
    assert_eq!(
        carve::to_html("> q\n+\n- item\n> more"),
        "<blockquote>\n  <p>q</p>\n  <ul>\n    <li>item</li>\n  </ul>\n  <p>more</p>\n</blockquote>"
    );
}

#[test]
fn blank_line_before_marker_keeps_it_literal() {
    assert_eq!(
        carve::to_html("> q\n\n+\n- item"),
        "<blockquote><p>q</p></blockquote>\n<p>+\n- item</p>"
    );
}

#[test]
fn indented_marker_is_not_a_continuation_marker() {
    // The indented `+` is not a continuation marker, so it folds into the open
    // quoted paragraph as literal text. The trailing `- item` list marker folds
    // in too (a list marker does not interrupt an open quoted paragraph, the
    // same as at the top level), so the whole thing is one quoted paragraph.
    assert_eq!(
        carve::to_html("> q\n  +\n- item"),
        "<blockquote><p>q\n+\n- item</p></blockquote>"
    );
}

#[test]
fn nested_marker_uses_the_outer_quotes_stripped_column() {
    assert_eq!(
        carve::to_html("> > q\n> +\n> para\n"),
        "<blockquote>\n  <blockquote>\n    <p>q</p>\n    <p>para</p>\n  </blockquote>\n</blockquote>"
    );
}

#[test]
fn nested_marker_attaches_whole_paragraph() {
    assert_eq!(
        carve::to_html("> > q\n> +\n> para\n> more\n"),
        "<blockquote>\n  <blockquote>\n    <p>q</p>\n    <p>para\nmore</p>\n  </blockquote>\n</blockquote>"
    );
}

#[test]
fn nested_marker_attaches_whole_list() {
    assert_eq!(
        carve::to_html("> > q\n> +\n> - a\n> - b\n"),
        "<blockquote>\n  <blockquote>\n    <p>q</p>\n    <ul>\n      <li>a</li>\n      <li>b</li>\n    </ul>\n  </blockquote>\n</blockquote>"
    );
}

#[test]
fn nested_marker_attaches_fenced_code_without_extra_space() {
    assert_eq!(
        carve::to_html("> > q\n> +\n> ```\n> code\n> ```\n"),
        "<blockquote>\n  <blockquote>\n    <p>q</p>\n    <pre><code>code\n</code></pre>\n  </blockquote>\n</blockquote>"
    );
}

#[test]
fn dangling_marker_after_collected_note_remains_text() {
    assert_eq!(
        carve::to_html("> [^a]: n\n> +\n\npara\n"),
        "<blockquote><p>+</p></blockquote>\n<p>para</p>"
    );
}

#[test]
fn nested_marker_controls() {
    for (source, expected) in [
        (
            "> > q\n> > +\n\npara\n",
            "<blockquote>\n  <blockquote><p>q\n+</p></blockquote>\n</blockquote>\n<p>para</p>",
        ),
        (
            "> > q\n> +\n\npara\n",
            "<blockquote>\n  <blockquote><p>q\n+</p></blockquote>\n</blockquote>\n<p>para</p>",
        ),
        (
            "> q\n+\npara\n",
            "<blockquote>\n  <p>q</p>\n  <p>para</p>\n</blockquote>",
        ),
        (
            "> +\n\npara\n",
            "<blockquote><p>+</p></blockquote>\n<p>para</p>",
        ),
        (
            "> - a\n> +\n> para\n",
            "<blockquote>\n  <ul>\n    <li>a\n      para\n    </li>\n  </ul>\n</blockquote>",
        ),
        ("> [^a]: n\n> +\n> para\n", "<blockquote>\n\n</blockquote>"),
        ("[^a]: n\n+\npara\n", ""),
        (
            "> > q\n> +\n> > r\n",
            "<blockquote>\n  <blockquote>\n    <p>q</p>\n    <blockquote><p>r</p></blockquote>\n  </blockquote>\n</blockquote>",
        ),
        (
            "> > > q\n> > +\n> > para\n",
            "<blockquote>\n  <blockquote>\n    <blockquote>\n      <p>q</p>\n      <p>para</p>\n    </blockquote>\n  </blockquote>\n</blockquote>",
        ),
        (
            "> > q\n>\n> +\n> para\n",
            "<blockquote>\n  <blockquote><p>q</p></blockquote>\n  <p>+\npara</p>\n</blockquote>",
        ),
    ] {
        assert_eq!(carve::to_html(source), expected, "{source:?}");
    }
}

#[test]
fn nested_marker_with_cli_options() {
    let options = carve::Options::new().with_positions(true);
    for (source, expected) in [
        ("> > q\n> +\n> para\n> more\n", "<blockquote>\n  <blockquote>\n    <p>q</p>\n    <p>para\nmore</p>\n  </blockquote>\n</blockquote>"),
        ("> > q\n> +\n> - a\n> - b\n", "<blockquote>\n  <blockquote>\n    <p>q</p>\n    <ul>\n      <li>a</li>\n      <li>b</li>\n    </ul>\n  </blockquote>\n</blockquote>"),
        ("> > q\n> +\npara\n", "<blockquote>\n  <blockquote><p>q\n+\npara</p></blockquote>\n</blockquote>"),
        ("> > q\n> +\n> > r\n", "<blockquote>\n  <blockquote>\n    <p>q</p>\n    <blockquote><p>r</p></blockquote>\n  </blockquote>\n</blockquote>"),
        ("x[^a]\n\n> [^a]: n\n> +\n> para\n", "<p>x<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<blockquote>\n\n</blockquote>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <p>n</p>\n      <p>para<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"),
    ] {
        assert_eq!(carve::try_to_html_with_options(source, &options).unwrap(), expected, "{source:?}");
        assert_eq!(carve::to_html(source), expected, "{source:?}");
    }
}

#[test]
#[ignore = "pending markup-carve/carve#3482: carve-js and the spec model disagree on this shape"]
fn nested_quotes_in_list_and_note_bodies_keep_existing_lazy_lines() {
    for (source, expected) in [
        (
            "- > > q\n  > +\n  para\n",
            "<ul>\n  <li>\n    <blockquote>\n      <blockquote><p>q\npara</p></blockquote>\n    </blockquote>\n  </li>\n</ul>",
        ),
        (
            "- > > q\n  > +\n  > para\n",
            "<ul>\n  <li>\n    <blockquote>\n      <blockquote><p>q\npara</p></blockquote>\n    </blockquote>\n  </li>\n</ul>",
        ),
        (
            "[^a]: > > q\n  > +\n  para\n\nx[^a]\n",
            "<p>x<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <blockquote>\n        <blockquote><p>q\npara</p></blockquote>\n      </blockquote>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>",
        ),
    ] {
        assert_eq!(carve::to_html(source), expected, "{source:?}");
    }
}
