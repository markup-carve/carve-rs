// markup-carve/carve#3450 (index 116): the item does not reach the quote, the outer dd does.
fn flat(source: &str) -> String {
    carve::to_html(source)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn the_ruled_shape_opens_the_quote_in_the_outer_description() {
    let html = flat(
        ":: term\n:  detail\n  1. ordered\n    :: term\n    :  detail\n      1. ordered\n       q\n       > quote\n",
    );
    assert!(
        html.ends_with("<ol> <li>ordered q &gt; quote</li> </ol> </dd> </dl>"),
        "{html}"
    );
}

#[test]
fn a_past_column_opener_interrupts_a_description_paragraph_holding_a_marker() {
    let quote = flat(":: t\n:  d\n   1. o\n    > quote\n");
    assert!(
        quote.contains("<p>d 1. o</p> <blockquote><p>quote</p></blockquote>"),
        "{quote}"
    );
    let heading = flat(":: t\n:  d\n   1. o\n    # h\n");
    assert!(
        heading.contains("<p>d 1. o</p> <h1 id=\"h\">h</h1>"),
        "{heading}"
    );
}

#[test]
fn a_list_marker_below_the_item_column_still_folds() {
    assert!(flat(":: t\n:  d\n   1. o\n    - b\n").contains("<dd>d 1. o - b</dd>"));
}
