// The canonical writer emits NO space between a code fence and its info string.
//
// `fenced_code_block` (resources/grammar.ebnf) states it for the writer while
// leaving the reader lenient:
//
// > The space between the fence and the info string is OPTIONAL (lenient:
// > both ```php and ``` php are accepted; Markdown writes no space, Djot
// > writes the space). The no-space form (```php) is canonical and is what
// > the X->Carve converters emit. It is a PADDING SLOT, not a marker
// > separator (PART 7)
//
// The writer emitted the Djot spelling instead, in every engine. Nothing
// caught it, and the reason it could not be caught by the existing checks is
// the first half of that same clause: the reader accepts both, so
// `parse(fmt(x)) == parse(x)`, `fmt(fmt(x)) == fmt(x)` and
// `to_html(fmt(x)) == to_html(x)` all hold either way. Only a BYTE assertion
// on the writer's output can tell the canonical form from the accepted one,
// which is what this file makes.
//
// ## Two slots meet on that line and only one of them moves
//
// The slot before the info string is `[space]`, optional, and canonically
// absent. The two slots INSIDE `code_fence_info` are `space+`, mandatory, so
// ```php"t" is not a fence opener at all and the separators between language,
// header and label stay exactly one space each.

use carve::{to_carve, to_html};

fn fmt(source: &str) -> String {
    to_carve(source)
}

/// (name, authored in the Djot spelling, canonical with no space)
fn shapes() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        ("a language only", "``` rust\nx\n```\n", "```rust\nx\n```\n"),
        (
            "a language and a quoted title",
            "``` rust \"src/main.rs\"\nx\n```\n",
            "```rust \"src/main.rs\"\nx\n```\n",
        ),
        (
            "a language and a label",
            "``` rust [tab-a]\nx\n```\n",
            "```rust [tab-a]\nx\n```\n",
        ),
        (
            "a language, a title and a label",
            "``` rust \"src/main.rs\" [tab-a]\nx\n```\n",
            "```rust \"src/main.rs\" [tab-a]\nx\n```\n",
        ),
        (
            "a title with no language",
            "``` \"src/main.rs\"\nx\n```\n",
            "```\"src/main.rs\"\nx\n```\n",
        ),
        (
            "a label with no language",
            "``` [tab-a]\nx\n```\n",
            "```[tab-a]\nx\n```\n",
        ),
        // `raw_block` (PART 9 §20) spells its otherwise identical slot the same
        // way, so it is checked rather than assumed: the `=` after the slot
        // SELECTS a raw block over a code block, and the grammar permits leading
        // whitespace before it, so ``` =html reads as a raw block and would have
        // hidden the same defect.
        //
        // THIS ROW WAS ALREADY CORRECT. A raw block writes its own opener and
        // never went through the fence-info builder, so it passed before this
        // change and after it. It is kept as a check, not as a fix: it fails
        // when the slot is widened there, which is how the defect would arrive.
        (
            "a raw block",
            "``` =html\n<b>raw</b>\n```\n",
            "```=html\n<b>raw</b>\n```\n",
        ),
    ]
}

#[test]
fn the_djot_spelling_normalizes_to_the_canonical_one() {
    for (name, authored, canonical) in shapes() {
        assert_eq!(fmt(authored), canonical, "{name}");
    }
}

#[test]
fn the_canonical_spelling_is_a_fixed_point() {
    for (name, _authored, canonical) in shapes() {
        assert_eq!(fmt(canonical), canonical, "{name}");
    }
}

/// `fmt(fmt(x)) == fmt(x)`, stated rather than inferred.
#[test]
fn the_writer_settles() {
    for (name, authored, _canonical) in shapes() {
        let once = fmt(authored);
        assert_eq!(fmt(&once), once, "{name}");
    }
}

/// `to_html(fmt(x)) == to_html(x)`. Holds in both states, which is exactly why
/// it could not catch this on its own.
#[test]
fn the_document_still_says_the_same_thing() {
    for (name, authored, _canonical) in shapes() {
        assert_eq!(to_html(&fmt(authored)), to_html(authored), "{name}");
    }
}

/// The control. A fence with NO info string has nothing to separate, so it is
/// the case that would expose a fix written as "drop one character after the
/// run".
#[test]
fn a_fence_with_no_info_string_neither_gains_nor_loses_a_space() {
    let source = "```\nx\n```\n";
    assert_eq!(fmt(source), source);
    assert_eq!(fmt(&fmt(source)), fmt(source));
}

/// A tilde fence reaches the writer as the same node - `code_block` records no
/// fence character (PART 12 §3) - so it comes back as backticks. That
/// normalization is pre-existing and is not what this file is about; it is
/// asserted so the row is not read as the slot rule failing to apply to tildes.
#[test]
fn a_tilde_fence_is_respelled_with_backticks_and_still_carries_no_space() {
    assert_eq!(fmt("~~~ rust\nx\n~~~\n"), "```rust\nx\n```\n");
}

/// Inside a container the writer emits the container's own prefix and then the
/// fence. The slot sits after the fence run, so the prefix is unaffected and the
/// fence is still tight against its language.
#[test]
fn the_rule_holds_under_a_container_prefix() {
    let in_a_list = "- item\n\n  ``` rust\n  x\n  ```\n";
    let in_a_quote = "> quoted\n>\n> ``` rust\n> x\n> ```\n";

    let list = fmt(in_a_list);
    assert!(list.contains("```rust\n"), "{list:?}");
    assert!(!list.contains("``` rust"), "{list:?}");
    assert_eq!(to_html(&list), to_html(in_a_list));

    let quote = fmt(in_a_quote);
    assert!(quote.contains("> ```rust\n"), "{quote:?}");
    assert!(!quote.contains("``` rust"), "{quote:?}");
    assert_eq!(to_html(&quote), to_html(in_a_quote));
}

/// The slot the fix must NOT touch. These separators are `space+` inside
/// `code_fence_info`; removing one does not tighten the opener, it stops the
/// line being a fence opener and the run falls back to an inline code span (the
/// INVALID-FENCE FALLBACK). So the reader is checked here too: if a later change
/// were to join the parts without a separator, the writer would emit a document
/// that no longer holds a code block at all.
#[test]
fn the_separators_inside_the_info_string_are_not_the_same_slot() {
    assert!(!to_html("```rust\"t\"\nx\n```\n").contains("<pre"));
    assert!(!to_html("```rust[l]\nx\n```\n").contains("<pre"));

    let source = "```rust \"t\" [l]\nx\n```\n";
    assert_eq!(fmt(source), source);
}
