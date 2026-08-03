//! A fence on a list item's marker line is the item's FIRST content, so there
//! is no open paragraph for it to interrupt and the PART 9 §10 I4 closer
//! lookahead does not apply.
//!
//! carve-rs required a closer here, so `* ```` rendered as inline verbatim
//! while the top level and a block quote both opened a code block - the same
//! construct reading three different ways depending on its container
//! (carve-rs#458).

use carve::to_html;

fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn an_unterminated_fence_on_a_marker_line_opens_a_code_block() {
    assert_eq!(
        squash(&to_html("* ```")),
        "<ul> <li> <pre><code> </code></pre> </li> </ul>"
    );
    assert_eq!(
        squash(&to_html("- ```")),
        "<ul> <li> <pre><code> </code></pre> </li> </ul>"
    );
}

#[test]
fn the_three_containers_agree() {
    // The point of the fix: one construct, one reading. The top level and the
    // block quote were already right.
    // The space is the code block's trailing newline, squashed.
    let expected = "<pre><code> </code></pre>";

    assert_eq!(squash(&to_html("```")), expected);
    assert!(squash(&to_html("> ```")).contains(expected));
    assert!(squash(&to_html("* ```")).contains(expected));
}

#[test]
fn an_unterminated_marker_line_fence_takes_the_items_indented_body() {
    assert_eq!(
        squash(&to_html("* ```\n  x\n")),
        "<ul> <li> <pre><code>x </code></pre> </li> </ul>"
    );
}

#[test]
fn a_terminated_marker_line_fence_is_unchanged() {
    // The settled cases must not move: a closed fence, and a closed fence with
    // blank-separated trailing text that loosens the item.
    assert_eq!(
        squash(&to_html("- ```\n  c\n  ```\n")),
        "<ul> <li> <pre><code>c </code></pre> </li> </ul>"
    );
    assert_eq!(
        squash(&to_html("- ```\n  c\n  ```\n\n  tail\n")),
        "<ul> <li> <pre><code>c </code></pre> <p>tail</p> </li> </ul>"
    );
}
