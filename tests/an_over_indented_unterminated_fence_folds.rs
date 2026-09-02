//! AN UNTERMINATED CODE FENCE PAST A LIST ITEM'S CONTENT COLUMN IS PARAGRAPH
//! TEXT, exactly as it is at that column (markup-carve/carve-rs#1523).
//!
//! PART 9 §10 I4: a fenced code block interrupts an open paragraph only with a
//! matching closer ahead. `interrupts_paragraph` asks that at the content
//! column, so `- x` over `  ``` ` folds. The band PAST the column asked
//! `item_block_opener` instead, a pure shape test that cannot consult the
//! closer - so the same fence one column further out opened a `<pre>`, and the
//! engine answered one question two ways one column apart.
//!
//! ORACLE: the executable spec (`tests/spec/scripts/spec/layout.mjs` +
//! `html.mjs`) at carve `95fc3a04`, which is BOTH the pinned submodule and spec
//! main, so no pin/main split applies here. Every expectation below is that
//! oracle's own output. carve-js agrees on all of it (recorded on the ticket).

use carve::{to_html, to_html_with_options, Options};

fn both_paths(src: &str) -> String {
    let facade = to_html(src);
    let authoritative = to_html_with_options(src, &Options::default().with_positions(true));
    assert_eq!(
        facade, authoritative,
        "the library path and the position-tracking path disagree on {src:?}"
    );
    facade
}

const FOLDED: &str = "<ul>\n  <li>x\n<code></code></li>\n</ul>";

#[test]
fn the_reported_document_folds_the_fence() {
    assert_eq!(both_paths("- x\n   ```\n"), FOLDED);
}

#[test]
fn every_column_past_the_content_column_folds() {
    // The column is not the question - the missing closer is. One past the
    // content column and eight past it answer the same.
    for column in 3..=10 {
        let src = format!("- x\n{}```\n", " ".repeat(column));
        assert_eq!(both_paths(&src), FOLDED, "{src:?}");
    }
}

#[test]
fn the_at_column_spelling_is_unchanged() {
    // THE CONTROL THE DIVERGENCE WAS MEASURED AGAINST. This spelling already
    // folded; the fix is that the over-indented one now answers the same.
    assert_eq!(both_paths("- x\n  ```\n"), FOLDED);
}

#[test]
fn the_tilde_spelling_folds_too() {
    // A tilde run is not an inline code marker, so the fold is visible as the
    // literal characters rather than as an empty `<code>`.
    assert_eq!(
        both_paths("- x\n   ~~~\n"),
        "<ul>\n  <li>x\n~~~</li>\n</ul>"
    );
}

#[test]
fn an_info_string_folds_with_the_fence() {
    // It reached `class="language-z"` before, off a fence that never opened.
    assert_eq!(
        both_paths("- x\n   ```z\n"),
        "<ul>\n  <li>x\n<code>z</code></li>\n</ul>"
    );
}

#[test]
fn the_line_under_the_fence_folds_into_the_item() {
    // It was swallowed as code content.
    assert_eq!(
        both_paths("- x\n   ```\n   y\n"),
        "<ul>\n  <li>x\n<code>\ny</code></li>\n</ul>"
    );
}

#[test]
fn a_terminated_over_indented_fence_still_opens() {
    // THE CLOSER IS THE WHOLE CONDITION. Drop the closer probe from the new
    // guard - fold on `detect_fence_open` alone - and these three lose their
    // `<pre>`. The closer sits at the fence's own column, so a probe that
    // demands a DEEPER closer (`>` instead of `>=` on the residual indent)
    // fails here too.
    assert_eq!(
        both_paths("- x\n   ```\n   y\n   ```\n"),
        "<ul>\n  <li>x\n    <pre><code>y\n</code></pre>\n  </li>\n</ul>"
    );
    assert_eq!(
        both_paths("- x\n   ~~~\n   y\n   ~~~\n"),
        "<ul>\n  <li>x\n    <pre><code>y\n</code></pre>\n  </li>\n</ul>"
    );
    assert_eq!(
        both_paths("- x\n   ```z\n   y\n   ```\n"),
        "<ul>\n  <li>x\n    <pre><code class=\"language-z\">y\n</code></pre>\n  </li>\n</ul>"
    );
}

#[test]
fn every_other_over_indented_opener_still_opens() {
    // ONLY THE FENCE ARM MOVED. Widen the new guard to any `item_block_opener`
    // and these three fold into the item's text instead of opening their block
    // - the #1705 rebase this band exists for.
    for (line, opened) in [
        ("# h", "<h1 id=\"h\">h</h1>"),
        ("> q", "<blockquote><p>q</p></blockquote>"),
        (
            "| a |",
            "<table>\n      <tbody>\n        <tr><td>a</td></tr>\n      </tbody>\n    </table>",
        ),
    ] {
        let src = format!("- x\n   {line}\n");
        assert_eq!(
            both_paths(&src),
            format!("<ul>\n  <li>x\n    {opened}\n  </li>\n</ul>"),
            "{src:?}"
        );
    }
}

#[test]
fn the_fold_holds_inside_a_deeper_item_and_inside_a_quote() {
    assert_eq!(
        both_paths("- - x\n     ```\n"),
        "<ul>\n  <li>\n    <ul>\n      <li>x\n<code></code></li>\n    </ul>\n  </li>\n</ul>"
    );
    assert_eq!(
        both_paths("> - x\n>    ```\n"),
        "<blockquote>\n  <ul>\n    <li>x\n<code></code></li>\n  </ul>\n</blockquote>"
    );
}
