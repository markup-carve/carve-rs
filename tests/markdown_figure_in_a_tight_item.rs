//! A figure in a list item writes its target first, so the Markdown target
//! keeps the item tight when that target interrupts a paragraph, the same as
//! the bare quote or fence (markup-carve/carve-rs#2968, corpus 618 and 622).

use carve::to_markdown;

#[test]
fn a_captioned_quote_or_fence_stays_in_a_tight_item() {
    for (name, source, expected) in [
        (
            "618-10",
            "- text\n  > q\n\n  ^ cap\n- c\n",
            "- text\n  > q\n\n  cap\n- c\n",
        ),
        (
            "618-12",
            "- ^ cap\n  > q\n\n  ^ cap\n- c\n",
            "- ^ cap\n  > q\n\n  cap\n- c\n",
        ),
        (
            "618-14",
            "- ^ cap\n  > q\n\n  ^ cap\n  more\n- c\n",
            "- ^ cap\n  > q\n\n  cap\n  more\n- c\n",
        ),
        (
            "618-23",
            "- text\n\n  > q\n\n  ^ cap\n- c\n",
            "- text\n  > q\n\n  cap\n- c\n",
        ),
        (
            "618-26",
            "- text\n\n  > q\n\n  ^ cap\n",
            "- text\n  > q\n\n  cap\n",
        ),
        (
            "622-1",
            "- x\n  ```\n  x\n  ```\n\n  ^ cap\n- c\n",
            "- x\n  ```\n  x\n  ```\n  cap\n- c\n",
        ),
        (
            "622-2",
            "- x\n  ```\n  x\n  ```\n  ^ cap\n- c\n",
            "- x\n  ```\n  x\n  ```\n  cap\n- c\n",
        ),
        (
            "622-3",
            "- x\n  ~~~\n  x\n  ~~~\n  ^ cap\n- c\n",
            "- x\n  ```\n  x\n  ```\n  cap\n- c\n",
        ),
        (
            "622-5",
            "- x\n+\n```\nx\n```\n^ cap\n- c\n",
            "- x\n  ```\n  x\n  ```\n  cap\n- c\n",
        ),
        (
            "622-7",
            "1. x\n+\n~~~\nx\n~~~\n^ cap\n2. c\n",
            "1. x\n   ```\n   x\n   ```\n   cap\n2. c\n",
        ),
    ] {
        assert_eq!(to_markdown(source), expected, "{name}");
    }
}
