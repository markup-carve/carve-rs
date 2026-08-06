//! A `+` that is NOT consumed as a continuation marker is lazy text
//! (carve-rs#672, markup-carve/carve#812).
//!
//! The lead-paragraph loop broke on `trim_ascii(next) == "+"` at ANY indent. A
//! `+` acts as a marker only at or below the item's base column - an indented
//! one is not consumed by any of the three engines, all of which render it - so
//! breaking on it turned ordinary lazy text into a second block here and a soft
//! break in carve-js and carve-php.
//!
//! The marker case is unchanged, and the tests below pin both sides: a `+` at
//! column 0 still attaches the block after it, and an indented one now folds.

fn item_html(source: &str) -> String {
    let html = carve::to_html(source);
    let start = html.find("<ul>").expect("a list");
    let end = html.find("</ul>").expect("its end");
    html[start..end + 5].replace('\n', " ")
}

#[test]
fn an_indented_plus_folds_into_the_paragraph() {
    assert_eq!(item_html("- a\n  +\n\nx\n"), "<ul>   <li>a +</li> </ul>");
}

#[test]
fn a_column_zero_plus_still_attaches_the_block_after_it() {
    // The marker case. `b` is attached content, not part of `a`.
    assert_eq!(
        item_html("- a\n+\nb\n\nx\n"),
        "<ul>   <li>a     b   </li> </ul>"
    );
}

#[test]
fn a_column_zero_plus_still_attaches_a_quote() {
    let html = item_html("- a\n+\n> q\n\nx\n");
    assert!(
        html.contains("<blockquote>"),
        "the marker stopped attaching a block:\n{html}"
    );
}

#[test]
fn a_plain_continuation_line_is_unchanged() {
    // The control: this always folded, and the fix must not have reached it by
    // widening something.
    assert_eq!(item_html("- a\n  b\n\nx\n"), "<ul>   <li>a b</li> </ul>");
}

#[test]
fn an_indented_plus_leaves_one_block_in_the_tree() {
    // The HTML above could pass with two blocks that happen to render alike, so
    // this asserts the shape the other two engines build.
    //
    // Counted over the WHOLE document rather than by slicing the item out of the
    // JSON text: the document holds the item's one paragraph plus the trailing
    // `x`, so two. A slice-based count read the trailing one as the item's and
    // reported a failure the tree did not have.
    let json = carve::to_json(&carve::parse("- a\n  +\n\nx\n"));
    let paragraphs = json.matches("\"type\":\"paragraph\"").count();
    assert_eq!(
        paragraphs, 2,
        "expected the item's paragraph plus the trailing one:\n{json}"
    );
}
