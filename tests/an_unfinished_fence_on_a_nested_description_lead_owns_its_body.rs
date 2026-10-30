//! An unfinished nested marker fence releases below-column lines.

use carve::{to_html, to_html_with_options, Options};

fn flat(source: &str) -> String {
    let facade = to_html(source);
    let positions = to_html_with_options(source, &Options::default().with_positions(true));
    assert_eq!(facade, positions, "both-paths disagree on {source:?}"); // #908
    facade.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn the_fence_releases_the_flush_left_body_and_closer() {
    assert_eq!(
        flat(":: t\n: - ``` x\ncode\n```\n"),
        "<dl> <dt>t</dt> <dd> <ul> <li> \
         <pre><code class=\"language-x\"></code></pre> </li> </ul> </dd> </dl> <p>code</p> <pre><code></code></pre>"
    );
}

#[test]
fn a_tilde_fence_answers_the_same_way() {
    assert_eq!(
        flat(":: t\n: - ~~~ x\ncode\n~~~\n"),
        "<dl> <dt>t</dt> <dd> <ul> <li> \
         <pre><code class=\"language-x\"></code></pre> </li> </ul> </dd> </dl> <p>code</p> <pre><code></code></pre>"
    );
}

#[test]
fn a_fence_with_no_info_string_releases_its_body_too() {
    assert_eq!(
        flat(":: t\n: - ```\ncode\n```\n"),
        "<dl> <dt>t</dt> <dd> <ul> <li> \
         <pre><code></code></pre> </li> </ul> </dd> </dl> <p>code</p> <pre><code></code></pre>"
    );
}

#[test]
fn a_blank_line_ends_the_fence_and_a_new_entry_follows() {
    assert_eq!(
        flat(":: t\n: - ``` x\ncode\n\n:: t2\n: plain\n"),
        "<dl> <dt>t</dt> <dd> <ul> <li> \
         <pre><code class=\"language-x\"></code></pre> </li> </ul> </dd> </dl> <p>code</p> <dl> \
         <dt>t2</dt> <dd>plain</dd> </dl>"
    );
}

/// The single-item control is unchanged: a top-level item's own lead fence does
/// NOT own the flush-left body (corpus 276).
#[test]
fn the_single_item_control_is_unchanged() {
    assert_eq!(
        flat("- ``` x\ncode\n```\n"),
        "<ul> <li> <pre><code class=\"language-x\"></code></pre> </li> </ul> \
         <p>code</p> <pre><code></code></pre>"
    );
}
