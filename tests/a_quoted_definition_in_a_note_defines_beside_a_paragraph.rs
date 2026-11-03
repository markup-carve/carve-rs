//! A link definition interrupts a paragraph, so one on a quoted line of a
//! note body defines even after a quoted paragraph line (PART 9R R1a,
//! CARVE-P9R-002). A definition-shaped line inside a quoted code fence stays
//! code.

fn html(source: &str) -> String {
    carve::to_html(source)
}

#[test]
fn a_quoted_link_definition_after_a_quoted_paragraph_defines() {
    let out = html("[^n]: a\n  > prose\n  > [r]: /x\n[x][r] [^n]\n");
    assert!(out.contains("<a href=\"/x\">x</a>"), "{out}");
    assert!(out.contains("<blockquote><p>prose</p></blockquote>"), "{out}");
    assert!(!out.contains("[r]: /x"), "{out}");
}

#[test]
fn a_quoted_link_definition_after_a_quoted_list_item_defines() {
    let out = html("- [^n]: a\n   > - item\n   > [r]: /x\n\n[x][r] [^n]\n");
    assert!(out.contains("<a href=\"/x\">x</a>"), "{out}");
}

#[test]
fn a_definition_shaped_line_in_a_quoted_fence_stays_code() {
    let out = html("[^n]: a\n  > ```\n  > code\n  > [r]: /x\n\n[x][r] [^n]\n");
    assert!(out.contains("[x][r]"), "{out}");
    assert!(out.contains("code\n[r]: /x"), "{out}");
}

#[test]
fn a_deeper_quoted_fence_line_does_not_close_the_outer_fence() {
    let out = html("[^n]: a\n  > ```\n  > > ```\n  > prose\n  > [r]: /x\n\n[x][r] [^n]\n");
    assert!(out.contains("[x][r]"), "{out}");
    assert!(out.contains("prose\n[r]: /x"), "{out}");
}

#[test]
fn an_outer_quoted_fence_opened_as_a_nested_quote_ends_hides_the_definition() {
    let out = html("[^n]: a\n  > > ```\n  > ```\n  > [r]: /x\n\n[x][r] [^n]\n");
    assert!(out.contains("<p>[x][r] "), "{out}");
}
