fn html(source: &str) -> String {
    let plain = carve::to_html(source);
    assert_eq!(
        plain,
        carve::to_html_with_options(source, &carve::Options::default().with_positions(true)),
        "position path differs for {source:?}"
    );
    plain
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("> <", "><")
}

#[test]
fn folded_separator_stays_interruptible() {
    assert_eq!(
        html("- a\n   :: term\n  :  detail\n    :: term\n    :  detail\n      [^n]: note\n       q\n1. ordered\n"),
        "<ul><li>a <dl><dt>term</dt><dd>detail</dd><dt>term</dt><dd>detail</dd></dl></li></ul><ol><li>ordered</li></ol>"
    );
}

#[test]
fn separator_at_item_column_starts_item_text() {
    assert_eq!(
        html("- a\n   :: term\n  :  detail\n"),
        "<ul><li>a <dl><dt>term</dt><dd>detail</dd></dl></li></ul>"
    );
    assert_eq!(
        html("- a\n   :: term\n  :  detail\n  x\n"),
        "<ul><li>a <dl><dt>term</dt><dd>detail x</dd></dl></li></ul>"
    );
    assert_eq!(
        html("- a\n   :: term\n  :  detail\n  :: t2\n"),
        "<ul><li>a <dl><dt>term</dt><dd>detail</dd><dt>t2</dt></dl></li></ul>"
    );
    assert_eq!(
        html("- a\n    :: m\n   : l\n"),
        "<ul><li>a <dl><dt>m</dt><dd>l</dd></dl></li></ul>"
    );
    assert_eq!(
        html("- a\n  - b\n      :: m\n    : l\n"),
        "<ul><li>a <ul><li>b <dl><dt>m</dt><dd>l</dd></dl></li></ul></li></ul>"
    );
}

#[test]
fn separator_below_item_column_ends_the_item() {
    // A term is one line (carve#4861), so the separator below the item's
    // column is text after the list.
    assert_eq!(
        html("- a\n   :: m\n : l\n"),
        "<ul><li>a <dl><dt>m</dt></dl></li></ul><p>: l</p>"
    );
}
