//! The heading index is built in DOCUMENT ORDER (carve-rs#1186).
//!
//! `collect_explicit_ids` and `collect_heading_titles` run on every parse and
//! became worklists so that nesting depth costs heap rather than host stack. A
//! worklist is a stack, and a stack pops last-in-first, so two spellings of it
//! reverse the visit: pushing a block's children without first pushing the
//! REMAINDER of its level, and pushing sibling child slices in SOURCE order.
//!
//! What the reversal breaks is PART 11 R1 - an implicit `[Text][]` resolves to
//! the FIRST heading with that text, which is an `or_insert` on a
//! document-order walk.
//!
//! THE IDS CANNOT SHOW IT, which is why these shapes carry explicit ones. Two
//! headings with the same text and no `{#id}` get `Same` and `Same-2`, and a
//! reversed walk hands the same two strings to the opposite headings - so the
//! link's href is unchanged and the defect is invisible. Measured: reversing
//! the sibling push left every assertion on generated ids green. An authored
//! `{#alpha}` / `{#beta}` breaks the symmetry, and then the href names which
//! heading the index actually reached first.

fn html(source: &str) -> String {
    carve::to_html(source)
}

/// Sibling LIST ITEMS: the arm that has to push its child slices in reverse for
/// the pops to come out forwards.
#[test]
fn an_implicit_reference_takes_the_first_of_two_sibling_list_items() {
    let out = html("- {#alpha}\n  ## Same\n\n- {#beta}\n  ## Same\n\nSee [Same][].\n");
    assert!(
        out.contains("href=\"#alpha\""),
        "the implicit reference did not resolve to the FIRST heading:\n{out}"
    );
    assert!(
        !out.contains("href=\"#beta\""),
        "it resolved to the LAST heading instead:\n{out}"
    );
}

/// The REMAINDER of a level, which has to be pushed before a block's children.
/// A heading inside a container and a sibling heading after it is the shape
/// that separates pre-order from children-last.
#[test]
fn an_implicit_reference_takes_the_heading_inside_the_container_above_it() {
    let out = html(":::: note\n{#alpha}\n## Same\n::::\n\n{#beta}\n## Same\n\nSee [Same][].\n");
    assert!(
        out.contains("href=\"#alpha\""),
        "the container's heading was not reached first:\n{out}"
    );
    assert!(
        !out.contains("href=\"#beta\""),
        "the heading after the container was reached first:\n{out}"
    );
}

/// Two levels deep, so a walk that handled one level of nesting by accident
/// cannot pass.
#[test]
fn two_containers_deep_is_still_reached_before_what_follows_them() {
    let out = html(
        ":::: note\n::: note\n{#alpha}\n## Same\n:::\n::::\n\n{#beta}\n## Same\n\nSee [Same][].\n",
    );
    assert!(
        out.contains("href=\"#alpha\""),
        "the twice-nested heading was not reached first:\n{out}"
    );
    assert!(
        !out.contains("href=\"#beta\""),
        "the heading after the containers was reached first:\n{out}"
    );
}

/// A BEHAVIOR GUARD rather than an order one, said out loud so it is not read
/// as coverage it does not give: the rendered ids come from the renderer's own
/// numbering, so this holds under a reversed index walk too. It is here because
/// `collect_explicit_ids` feeds the skip and a conversion could drop an id from
/// the set entirely, which this WOULD catch.
#[test]
fn a_generated_id_skips_an_explicit_one_wherever_that_one_was_written() {
    let out = html("- ## Same\n\n- {#Same-2}\n  ## Same\n\n- ## Same\n");
    for id in ["id=\"Same\"", "id=\"Same-2\"", "id=\"Same-3\""] {
        assert!(out.contains(id), "{id} is missing:\n{out}");
    }
}
