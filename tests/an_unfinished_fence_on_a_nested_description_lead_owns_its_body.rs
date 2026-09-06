//! An unterminated code fence opened on a nested item's lead inside a
//! DESCRIPTION BODY owns its flush-left body (markup-carve/carve#1958, corpus
//! section 455; carve-rs#1559, and the description spelling of #1547).
//!
//! A fence's content is not re-scanned for structure, so the flush-left lines
//! below the fence - including a flush-left closing fence - are its verbatim
//! body, not the end of the item. The collector framed such a line at no column
//! so the re-parse's fence takes it.
//!
//! SCOPE: this is the corpus-pinned description spelling. The single-item lead
//! fence (corpus 276) already answered correctly, and the un-pinned nested-LIST
//! spelling (`- - ``` x`) and the closer-column edge variants remain
//! pre-existing-divergent - they are not pinned by the corpus.
//!
//! ORACLE: the executable spec (`scripts/spec/layout.mjs` + `html.mjs`) at spec
//! main; corpus section 455 pins these rows.

use carve::{to_html, to_html_with_options, Options};

fn flat(source: &str) -> String {
    let facade = to_html(source);
    let positions = to_html_with_options(source, &Options::default().with_positions(true));
    assert_eq!(facade, positions, "both-paths disagree on {source:?}"); // #908
    facade.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn the_fence_owns_the_flush_left_body_and_closer() {
    assert_eq!(
        flat(":: t\n: - ``` x\ncode\n```\n"),
        "<dl> <dt>t</dt> <dd> <ul> <li> \
         <pre><code class=\"language-x\">code ``` </code></pre> </li> </ul> </dd> </dl>"
    );
}

#[test]
fn a_tilde_fence_answers_the_same_way() {
    assert_eq!(
        flat(":: t\n: - ~~~ x\ncode\n~~~\n"),
        "<dl> <dt>t</dt> <dd> <ul> <li> \
         <pre><code class=\"language-x\">code ~~~ </code></pre> </li> </ul> </dd> </dl>"
    );
}

#[test]
fn a_fence_with_no_info_string_owns_its_body_too() {
    assert_eq!(
        flat(":: t\n: - ```\ncode\n```\n"),
        "<dl> <dt>t</dt> <dd> <ul> <li> \
         <pre><code>code ``` </code></pre> </li> </ul> </dd> </dl>"
    );
}

#[test]
fn a_blank_line_ends_the_fence_and_a_new_entry_follows() {
    assert_eq!(
        flat(":: t\n: - ``` x\ncode\n\n:: t2\n: plain\n"),
        "<dl> <dt>t</dt> <dd> <ul> <li> \
         <pre><code class=\"language-x\">code </code></pre> </li> </ul> </dd> \
         <dt>t2</dt> <dd>plain</dd> </dl>"
    );
}

/// The single-item control is unchanged: a top-level item's own lead fence does
/// NOT own the flush-left body (corpus 276).
#[test]
fn the_single_item_control_is_unchanged() {
    assert_eq!(
        flat("- ``` x\ncode\n```\n"),
        "<ul> <li> <pre><code class=\"language-x\"> </code></pre> </li> </ul> \
         <p>code <code></code></p>"
    );
}
