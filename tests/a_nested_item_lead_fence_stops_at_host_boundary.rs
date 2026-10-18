//! A fence on a nested marker line owns only lines that reach its host.
//!
//! A flush-left line supplies no nested item prefix (CARVE-P0-013). Earlier
//! tests here pinned a Rust-only reading that framed such a line as code
//! payload. The container differential found the disagreement on `- - ``` `
//! followed by `tail`; the spec, JS and PHP place `tail` outside the list.

use carve::{to_html, to_html_with_options, to_json_with_options, Options};

fn flat(source: &str) -> String {
    let facade = to_html(source);
    let positions = to_html_with_options(source, &Options::default().with_positions(true));
    assert_eq!(facade, positions, "both-paths disagree on {source:?}"); // #908
    facade.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ---- The nested item's code block ends at its host boundary ----

#[test]
fn a_nested_lead_fence_does_not_own_a_flush_left_body_or_closer() {
    assert_eq!(
        flat("- - ``` x\ncode\n```\n"),
        "<ul> <li> <ul> <li> \
         <pre><code class=\"language-x\"></code></pre> </li> </ul> </li> </ul> <p>code <code></code></p>",
    );
}

#[test]
fn a_below_column_body_is_outside_the_fence() {
    assert_eq!(
        flat("- - ``` x\n code\n ```\n"),
        "<ul> <li> <ul> <li> \
         <pre><code class=\"language-x\"></code></pre> </li> </ul> </li> </ul> <p>code <code></code></p>",
    );
}

#[test]
fn a_three_deep_ladder_owns_the_body_at_the_innermost_item() {
    assert_eq!(
        flat("- - - ``` x\ncode\n```\n"),
        "<ul> <li> <ul> <li> <ul> <li> \
         <pre><code class=\"language-x\"></code></pre> </li> </ul> </li> </ul> </li> </ul> <p>code <code></code></p>",
    );
}

#[test]
fn a_quoted_item_fence_requires_marked_body_lines() {
    assert_eq!(
        flat("> - ``` x\ncode\n```\n"),
        "<blockquote> <ul> <li> <pre><code class=\"language-x\"></code></pre> </li> </ul> </blockquote> <p>code <code></code></p>",
    );
}

#[test]
fn an_ordered_ladder_owns_the_body() {
    assert_eq!(
        flat("1. 1. ``` x\ncode\n```\n"),
        "<ol> <li> <ol> <li> \
         <pre><code class=\"language-x\"></code></pre> </li> </ol> </li> </ol> <p>code <code></code></p>",
    );
}

#[test]
fn a_tilde_fence_answers_the_same_way() {
    assert_eq!(
        flat("- - ~~~ x\ncode\n~~~\n"),
        "<ul> <li> <ul> <li> \
         <pre><code class=\"language-x\"></code></pre> </li> </ul> </li> </ul> <p>code ~~~</p>",
    );
}

#[test]
fn a_fence_with_no_info_string_owns_its_body() {
    assert_eq!(
        flat("- - ```\ncode\n```\n"),
        "<ul> <li> <ul> <li> <pre><code></code></pre> </li> </ul> </li> </ul> <p>code <code></code></p>",
    );
}

#[test]
fn an_unterminated_fence_ends_with_its_nested_host() {
    assert_eq!(
        flat("- - ``` x\ncode\n"),
        "<ul> <li> <ul> <li> \
         <pre><code class=\"language-x\"></code></pre> </li> </ul> </li> </ul> <p>code</p>",
    );
}

#[test]
fn a_lone_flush_left_run_opens_a_document_block() {
    assert_eq!(
        flat("- - ``` x\n```\n"),
        "<ul> <li> <ul> <li> \
         <pre><code class=\"language-x\"></code></pre> </li> </ul> </li> </ul> <pre><code></code></pre>",
    );
}

#[test]
fn a_trailing_sibling_marker_ends_the_fence_and_opens_an_outer_item() {
    assert_eq!(
        flat("- - ``` x\ncode\n```\n- lazy\n"),
        "<ul> <li> <ul> <li> \
         <pre><code class=\"language-x\"></code></pre> </li> </ul> </li> </ul> \
         <p>code <code> - lazy</code></p>",
    );
}

// A nested marker-line colon opener opens even without an indented body.

#[test]
fn a_nested_lead_div_with_a_flush_left_body_opens_an_empty_container() {
    assert_eq!(
        flat("- - ::: d\nbody\n:::\n"),
        "<ul> <li> <ul> <li> <div class=\"d\"> </div> </li> </ul> </li> </ul> <p>body</p> <div> </div>",
    );
}

#[test]
fn a_nested_lead_div_without_a_closer_opens_an_empty_container() {
    assert_eq!(
        flat("- - ::: d\nbody\n"),
        "<ul> <li> <ul> <li> <div class=\"d\"> </div> </li> </ul> </li> </ul> <p>body</p>",
    );
}

#[test]
fn a_nested_lead_admonition_with_a_flush_left_body_opens_an_empty_container() {
    assert_eq!(
        flat("- - ::: note\nbody\n:::\n"),
        "<ul> <li> <ul> <li> <aside class=\"admonition note\" aria-label=\"Note\"> </aside> </li> </ul> </li> </ul> <p>body</p> <div> </div>",
    );
}

#[test]
fn a_nested_lead_div_opener_alone_still_opens_an_empty_container() {
    // No following flush-left line to fold, so the opener stands (carve-rs#511
    // item 4): an empty div, exactly as the single-level `- ::: d` does.
    assert_eq!(
        flat("- - ::: d\n"),
        "<ul> <li> <ul> <li> <div class=\"d\"> </div> </li> </ul> </li> </ul>",
    );
}

// ---- controls: these already agreed and must stay put ----

#[test]
fn a_single_level_lead_fence_is_unchanged() {
    // At the OUTERMOST level the flush-left body is document-column-0 text: the
    // fence renders empty and the body leaks. Correct here, wrong only nested.
    assert_eq!(
        flat("- ``` x\ncode\n```\n"),
        "<ul> <li> <pre><code class=\"language-x\"></code></pre> </li> </ul> \
         <p>code <code></code></p>",
    );
}

#[test]
fn a_single_level_lead_div_is_unchanged() {
    assert_eq!(
        flat("- ::: d\nbody\n:::\n"),
        "<ul> <li> <div class=\"d\"> </div> </li> </ul> <p>body</p> <div> </div>",
    );
}

#[test]
fn a_body_at_the_content_column_stays_inside_the_fence() {
    assert_eq!(
        flat("- - ``` x\n    code\n    ```\n"),
        "<ul> <li> <ul> <li> \
         <pre><code class=\"language-x\">code </code></pre> </li> </ul> </li> </ul>",
    );
}

// ---- span bounds: the framed fence body must not push offsets past the file ----
//
// The lazy frame prepends codepoints the source never held; forwarding the raw
// source column would push every span ending on a framed line past the document
// length (carve-rs#1559). The HTML is identical either way, so only a position
// check catches it.

fn max_end_offset(source: &str) -> usize {
    let json = to_json_with_options(source, &Options::default().with_positions(true));
    let mut max = 0usize;
    let mut rest = json.as_str();
    while let Some(i) = rest.find("\"end_offset\":") {
        rest = &rest[i + "\"end_offset\":".len()..];
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        if let Ok(v) = digits.parse::<usize>() {
            max = max.max(v);
        }
    }
    max
}

#[test]
fn framed_fence_body_offsets_stay_within_the_source() {
    for source in [
        "- - ``` x\ncode\n```\n",
        "- - ``` x\n code\n ```\n",
        "- - - ``` x\ncode\n```\n",
        "> - ``` x\ncode\n```\n",
        "- - ~~~ x\ncode\n~~~\n",
        "- - ``` x\ncode\n",
        "- - ``` x\ncode\n```\n- lazy\n",
    ] {
        let len = source.chars().count();
        assert!(
            max_end_offset(source) <= len,
            "an offset ran past the {len}-char source for {source:?}",
        );
    }
}
