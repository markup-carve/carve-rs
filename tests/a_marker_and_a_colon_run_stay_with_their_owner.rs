//! A line that misses a description folds as text (PART 0 S2, S4), an invalid
//! colon opener absorbs the bare run under it past the item's column too, and a
//! div's own closer ends the div in a quote (CARVE-P9-016).

use carve::to_html;

fn html(source: &str) -> String {
    to_html(source)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn a_marker_missing_a_description_folds() {
    assert_eq!(
        html(":: t\n: - a\n- b\n"),
        "<dl> <dt>t</dt> <dd> <ul> <li>a - b</li> </ul> </dd> </dl>"
    );
    assert_eq!(
        html(":: t\n: - a\n\n- b\n"),
        "<dl> <dt>t</dt> <dd> <ul> <li>a</li> </ul> </dd> </dl> <ul> <li>b</li> </ul>"
    );
}

#[test]
fn an_indented_invalid_opener_absorbs_the_bare_run() {
    assert_eq!(
        html("- m\n   :::e\n   :::\nl\n"),
        "<ul> <li>m :::e ::: l</li> </ul>"
    );
}

#[test]
fn a_div_closer_in_a_quote_ends_the_div() {
    assert_eq!(
        html("> :::\n> :::e\n> :::\nl\n"),
        "<blockquote> <div> <p>:::e</p> </div> </blockquote> <p>l</p>"
    );
}

#[test]
fn a_longer_run_in_a_quoted_div_stays_text() {
    assert_eq!(
        html("> :::\n> ::::e\n> ::::\n> ::::\nl\n"),
        "<blockquote> <div> <p>::::e :::: :::: l</p> </div> </blockquote>"
    );
}
