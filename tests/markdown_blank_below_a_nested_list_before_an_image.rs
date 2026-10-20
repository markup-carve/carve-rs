//! CARVE-P11-047: a block below a nested list that opens with plain text would
//! be read as lazy text of the last sublist item, so the Markdown target keeps
//! the blank line that holds it as a block (markup-carve/carve-rs#2973).

use carve::to_markdown;

#[test]
fn an_image_or_figure_below_a_nested_list_keeps_its_blank() {
    for (name, source, expected) in [
        (
            "618-18",
            "- - text\n  ::: figure\n  ![a](b.png)\n  :::\n\n  ^ cap\n- c\n",
            "- - text\n\n  ![a](b.png)\n\n  **cap**\n- c\n",
        ),
        (
            "618-32",
            "- - text\n\n  ![i](x.png)\n\n  ^ cap\n- c\n",
            "- - text\n\n  ![i](x.png)\n  cap\n\n- c\n",
        ),
        (
            "618-33",
            "- - text\n\n  ![i](x.png)\n  ^ cap\n- c\n",
            "- - text\n\n  ![i](x.png)\n  cap\n\n- c\n",
        ),
        (
            "image at depth 2",
            "- - text\n\n  ![i](x.png)\n- c\n",
            "- - text\n\n  ![i](x.png)\n\n- c\n",
        ),
        (
            "image at depth 3",
            "- - - text\n\n    ![i](x.png)\n  - mid\n- c\n",
            "- - - text\n\n    ![i](x.png)\n\n  - mid\n- c\n",
        ),
        (
            "paragraph at depth 2",
            "- - text\n\n  after\n- c\n",
            "- - text\n\n  after\n\n- c\n",
        ),
        (
            "table",
            "- - text\n\n  | a |\n  | - |\n  | b |\n- c\n",
            "- - text\n\n  | a |\n  | --- |\n  | b |\n- c\n",
        ),
        (
            "paragraph at depth 3",
            "- - - text\n\n    after\n- c\n",
            "- - - text\n\n    after\n- c\n",
        ),
    ] {
        assert_eq!(to_markdown(source), expected, "{name}");
    }
}

#[test]
fn a_block_that_interrupts_a_paragraph_stays_glued() {
    assert_eq!(
        to_markdown("- - text\n\n  # h\n- c\n"),
        "- - text\n  # h\n- c\n"
    );
}

#[test]
fn a_thematic_break_below_a_nested_list_needs_no_blank() {
    // A setext underline cannot be lazy text, matching carve-js and carve-php.
    assert_eq!(
        to_markdown("- - text\n\n  ---\n- c\n"),
        "- - text\n  ---\n- c\n"
    );
}
