//! An unfinished code fence, or a colon container, opened on a NESTED list
//! item's marker lead line and its flush-left body (markup-carve/carve#1900,
//! carve-rs#1547 and #1548).
//!
//! A fence's content is not re-scanned for structure, so the flush-left lines
//! below a fence opened on a nested item's lead - including a flush-left closing
//! fence - are its verbatim body. And a colon container opened on such a lead,
//! whose body sits flush-left below its content column, is ITEM TEXT: the
//! opener does not open a container, the flush-left line folds in with it, and a
//! flush-left closing run publishes a separate document-level div.
//!
//! The single-level spellings (`- ``` x`, `- ::: d`) already answered
//! correctly - the outer item collector sees the following line there. Once the
//! item is nested, the inner re-parse used to see no following line at all, so
//! neither arm could fold or own it (carve-rs#1547/#1548).
//!
//! ORACLE: the executable spec (`scripts/spec/layout.mjs` + `html.mjs`) at spec
//! main (`1b27b68a`).

use carve::{to_html, to_html_with_options, to_json_with_options, Options};

fn flat(source: &str) -> String {
    let facade = to_html(source);
    let positions = to_html_with_options(source, &Options::default().with_positions(true));
    assert_eq!(facade, positions, "both-paths disagree on {source:?}"); // #908
    facade.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ---- #1547: the nested-item lead fence owns its flush-left body ----

#[test]
fn a_nested_lead_fence_owns_its_flush_left_body_and_closer() {
    assert_eq!(
        flat("- - ``` x\ncode\n```\n"),
        "<ul> <li> <ul> <li> \
         <pre><code class=\"language-x\">code ``` </code></pre> </li> </ul> </li> </ul>",
    );
}

#[test]
fn a_below_column_body_is_still_the_fences() {
    assert_eq!(
        flat("- - ``` x\n code\n ```\n"),
        "<ul> <li> <ul> <li> \
         <pre><code class=\"language-x\">code ``` </code></pre> </li> </ul> </li> </ul>",
    );
}

#[test]
fn a_three_deep_ladder_owns_the_body_at_the_innermost_item() {
    assert_eq!(
        flat("- - - ``` x\ncode\n```\n"),
        "<ul> <li> <ul> <li> <ul> <li> \
         <pre><code class=\"language-x\">code ``` </code></pre> </li> </ul> </li> </ul> </li> </ul>",
    );
}

#[test]
fn a_quoted_item_lead_fence_owns_its_body() {
    assert_eq!(
        flat("> - ``` x\ncode\n```\n"),
        "<blockquote> <ul> <li> \
         <pre><code class=\"language-x\">code ``` </code></pre> </li> </ul> </blockquote>",
    );
}

#[test]
fn an_ordered_ladder_owns_the_body() {
    assert_eq!(
        flat("1. 1. ``` x\ncode\n```\n"),
        "<ol> <li> <ol> <li> \
         <pre><code class=\"language-x\">code ``` </code></pre> </li> </ol> </li> </ol>",
    );
}

#[test]
fn a_tilde_fence_answers_the_same_way() {
    assert_eq!(
        flat("- - ~~~ x\ncode\n~~~\n"),
        "<ul> <li> <ul> <li> \
         <pre><code class=\"language-x\">code ~~~ </code></pre> </li> </ul> </li> </ul>",
    );
}

#[test]
fn a_fence_with_no_info_string_owns_its_body() {
    assert_eq!(
        flat("- - ```\ncode\n```\n"),
        "<ul> <li> <ul> <li> <pre><code>code ``` </code></pre> </li> </ul> </li> </ul>",
    );
}

#[test]
fn an_unterminated_fence_with_no_closer_runs_to_the_end() {
    assert_eq!(
        flat("- - ``` x\ncode\n"),
        "<ul> <li> <ul> <li> \
         <pre><code class=\"language-x\">code </code></pre> </li> </ul> </li> </ul>",
    );
}

#[test]
fn a_lone_flush_left_closer_is_the_fences_body() {
    assert_eq!(
        flat("- - ``` x\n```\n"),
        "<ul> <li> <ul> <li> \
         <pre><code class=\"language-x\">``` </code></pre> </li> </ul> </li> </ul>",
    );
}

#[test]
fn a_trailing_sibling_marker_ends_the_fence_and_opens_an_outer_item() {
    assert_eq!(
        flat("- - ``` x\ncode\n```\n- lazy\n"),
        "<ul> <li> <ul> <li> \
         <pre><code class=\"language-x\">code ``` </code></pre> </li> </ul> </li> \
         <li>lazy</li> </ul>",
    );
}

// ---- #1548: a nested-item lead colon container is item text ----

#[test]
fn a_nested_lead_div_with_a_flush_left_body_is_item_text() {
    assert_eq!(
        flat("- - ::: d\nbody\n:::\n"),
        "<ul> <li> <ul> <li>::: d body</li> </ul> </li> </ul> <div> </div>",
    );
}

#[test]
fn a_nested_lead_div_without_a_closer_is_item_text() {
    assert_eq!(
        flat("- - ::: d\nbody\n"),
        "<ul> <li> <ul> <li>::: d body</li> </ul> </li> </ul>",
    );
}

#[test]
fn a_nested_lead_admonition_with_a_flush_left_body_is_item_text() {
    assert_eq!(
        flat("- - ::: note\nbody\n:::\n"),
        "<ul> <li> <ul> <li>::: note body</li> </ul> </li> </ul> <div> </div>",
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
        "<ul> <li> <pre><code class=\"language-x\"> </code></pre> </li> </ul> \
         <p>code <code></code></p>",
    );
}

#[test]
fn a_single_level_lead_div_is_unchanged() {
    assert_eq!(
        flat("- ::: d\nbody\n:::\n"),
        "<ul> <li>::: d body</li> </ul> <div> </div>",
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
