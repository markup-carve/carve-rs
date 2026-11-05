fn writes(source: &str, expected: &str) {
    let out = carve::render_carve(&carve::parse(source)).unwrap();
    assert_eq!(out, expected, "{source:?}");
    assert_eq!(carve::to_html(&out), carve::to_html(source), "{source:?}");
    assert_eq!(
        carve::render_carve(&carve::parse(&out)).unwrap(),
        out,
        "{source:?}"
    );
}

// markup-carve/carve#5177: a `+` below a caption is refused, so the block
// under a captioned fence, quote or math block stays at the content column.
#[test]
fn a_block_below_a_captioned_fence_stays_at_the_content_column() {
    writes(
        "- text\n  ```\n  code\n  ```\n\n  ^ cap\n  > r\n",
        "- text\n  ```\n  code\n  ```\n  ^ cap\n  > r\n",
    );
}

#[test]
fn captioned_quote_and_math_figures_keep_their_follower() {
    for source in [
        "- text\n  > q\n  ^ cap\n  > r\n",
        "1. text\n   $$\n   x\n   $$\n   ^ cap\n   > r\n",
        "- a\n\n  - text\n    ```\n    code\n    ```\n    ^ cap\n    > r\n",
    ] {
        writes(source, source);
    }
}

#[test]
fn a_caption_that_ends_the_item_keeps_the_plus_spelling() {
    writes(
        "- x\n  ```\n  x\n  ```\n  ^ cap\n- c\n",
        "- x\n+\n```\nx\n```\n^ cap\n- c\n",
    );
}

#[test]
fn no_plus_for_a_running_caption_or_a_figure_below_a_figure() {
    for source in [
        "- text\n  ```\n  code\n  ```\n  ^ cap\n  more\n",
        "- a\n\n  - text\n    ```\n    x\n    ```\n    ^ cap\n    ```\n    y\n    ```\n    ^ cap2\n",
    ] {
        writes(source, source);
    }
}

#[test]
fn a_captioned_table_does_not_merge_with_the_table_below() {
    let source = "- outer\n\n  - one\n    | a | b |\n    ^ cap\n    | t |\n";
    writes(source, source);
}

// carve-rs#3876: the comment below a captioned quote stays in the tight item.
#[test]
fn a_comment_below_a_captioned_quote_keeps_the_nested_list_tight() {
    writes(
        "- x\n  - start\n    > q\n\n    ^ cap\n    %% note\n  - end\n",
        "- x\n  - start\n    > q\n    ^ cap\n    %% note\n  - end\n",
    );
    let source = "- x\n  - start\n    > q\n    ^ cap\n    %% note\n  - end\n";
    writes(source, source);
}
