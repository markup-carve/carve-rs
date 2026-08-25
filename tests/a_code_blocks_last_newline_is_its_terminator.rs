//! THE LAST NEWLINE BEFORE `</code>` IS THE LINE'S TERMINATOR, NOT A LINE
//! (markup-carve/carve#1708).
//!
//! A code block's content is bytes the author wrote, so gaining a line is a
//! CONTENT change and not a formatting one - and it happened in every mode,
//! with nothing reported, including the mode whose whole job is fidelity.
//!
//! THE RENDERER SETTLES IT RATHER THAN TASTE. `render_html` writes exactly one
//! newline before the closing tag for a code block whose content is `x`, and
//! two for one whose content ends in a blank line. An importer that strips
//! none does not invert its own renderer; one that strips them all makes the
//! two documents indistinguishable and loses the line the author wrote. Only
//! removing exactly one is the inverse, which is what `roundtrip` means when
//! its input is the engine's own output.
//!
//! Trailing spaces and tabs are content for the same reason, so the rule is
//! over the NEWLINE alone and a trim is not it.
//!
//! Nothing is reported, in any mode: the byte removed was the terminator.

use carve::html_import::{html_to_carve, HtmlImportMode, HtmlImportOptions};
use carve::render_html;

fn import(html: &str, mode: HtmlImportMode) -> (String, usize) {
    let options = HtmlImportOptions {
        mode,
        ..Default::default()
    };
    let result = html_to_carve(html, &options).unwrap();
    let count = result.report.diagnostics.len();
    (result.value, count)
}

const MODES: [HtmlImportMode; 3] = [
    HtmlImportMode::Safe,
    HtmlImportMode::Semantic,
    HtmlImportMode::Roundtrip,
];

#[test]
fn the_terminator_newline_does_not_become_a_blank_line() {
    for mode in MODES {
        let (carve, reported) = import("<pre><code>x\n</code></pre>", mode);
        assert_eq!(carve, "```\nx\n```\n", "{mode:?}");
        assert_eq!(
            reported, 0,
            "{mode:?}: nothing was lost, so nothing is said"
        );
    }
}

#[test]
fn a_newline_past_the_terminator_is_content() {
    // EXACTLY ONE, never a trim. This is the renderer's spelling for a block
    // whose content really does end in a blank line, and the one that a trim
    // makes indistinguishable from the case above.
    for mode in MODES {
        let (carve, reported) = import("<pre><code>x\n\n</code></pre>", mode);
        assert_eq!(carve, "```\nx\n\n```\n", "{mode:?}");
        assert_eq!(reported, 0, "{mode:?}");
    }
}

#[test]
fn two_newlines_past_the_terminator_are_two_lines() {
    let (carve, _) = import("<pre><code>x\n\n\n</code></pre>", HtmlImportMode::Roundtrip);
    assert_eq!(carve, "```\nx\n\n\n```\n");
}

#[test]
fn a_block_with_no_terminator_keeps_its_only_line() {
    // Nothing to strip. The rule removes a newline that is THERE; it does not
    // reach into content that has none.
    let (carve, _) = import("<pre><code>x</code></pre>", HtmlImportMode::Roundtrip);
    assert_eq!(carve, "```\nx\n```\n");
}

#[test]
fn a_pre_with_no_code_child_follows_the_same_rule() {
    let (carve, _) = import("<pre>x\n</pre>", HtmlImportMode::Roundtrip);
    assert_eq!(carve, "```\nx\n```\n");
    let (carve, _) = import("<pre>x\n\n</pre>", HtmlImportMode::Roundtrip);
    assert_eq!(carve, "```\nx\n\n```\n");
}

#[test]
fn trailing_spaces_on_the_last_line_are_content() {
    // THE RULE IS OVER THE NEWLINE ALONE. A trim would take these too, and
    // trailing whitespace inside a code block is bytes the author wrote.
    let (carve, _) = import("<pre><code>x  \n</code></pre>", HtmlImportMode::Roundtrip);
    assert_eq!(carve, "```\nx  \n```\n");
}

#[test]
fn a_multi_line_block_loses_no_line() {
    let (carve, _) = import("<pre><code>a\nb\n</code></pre>", HtmlImportMode::Roundtrip);
    assert_eq!(carve, "```\na\nb\n```\n");
}

#[test]
fn this_engines_own_html_imports_back_to_the_source_it_came_from() {
    // THE PROPERTY THE RULE EXISTS FOR, checked against the renderer rather
    // than against a hand-written expectation: `roundtrip` reads HTML this
    // engine produced, so the import has to be the renderer's inverse for
    // every one of these, and gaining a line each pass is the failure.
    for source in [
        "```\nx\n```\n",
        "```\nx\n\n```\n",
        "```\na\nb\n```\n",
        "```rust\nlet x = 1;\n```\n",
    ] {
        let document = carve::parse(source);
        let html = render_html(&document).unwrap();
        let (back, _) = import(&html, HtmlImportMode::Roundtrip);
        assert_eq!(back, source, "html was {html:?}");
    }
}
