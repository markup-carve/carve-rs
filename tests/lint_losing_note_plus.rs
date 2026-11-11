//! carve#5953 ruling A: a refused marker-line `+` on a losing duplicate footnote
//! definition is reported, and a following definition is the host's sibling.

use carve::{lint_carve, to_html};

fn findings(source: &str) -> Vec<String> {
    lint_carve(source)
        .into_iter()
        .filter(|w| w.rule.contains("continuation-marker") || w.rule == "duplicate-footnote-definition")
        .map(|w| format!("{}:{} {}", w.line, w.column, w.rule))
        .collect()
}

#[test]
fn a_losing_plus_and_the_winner_before_it_are_refused() {
    let source = "[^n]: +\n[^n]: +\n\n  para\n\nsee[^n]\n";
    assert_eq!(
        findings(source),
        [
            "1:7 continuation-marker-refused",
            "2:1 duplicate-footnote-definition",
            "2:7 continuation-marker-refused",
        ]
    );
    assert!(to_html(source).contains("<p>para<a href=\"#fnref1\""));
}

#[test]
fn a_losing_plus_before_a_right_shifted_line_is_refused() {
    let source = "[^a]: x\n[^a]: +\n flush\n\nsee[^a]\n";
    assert_eq!(
        findings(source),
        [
            "2:1 duplicate-footnote-definition",
            "2:7 continuation-marker-refused",
        ]
    );
}

#[test]
fn the_rule_holds_in_a_quote() {
    let source = "> [^n]: +\n> [^n]: +\n>\n>   para\n\nsee[^n]\n";
    assert_eq!(
        findings(source),
        [
            "1:9 continuation-marker-refused",
            "2:3 duplicate-footnote-definition",
            "2:9 continuation-marker-refused",
        ]
    );
}

#[test]
fn a_following_definition_with_an_indented_body_is_a_sibling() {
    let source = "[^n]: +\n[^m]: +\n  para\n\nsee[^n][^m]\n";
    let lint = lint_carve(source);
    let first = lint.iter().find(|w| w.line == 1).expect("line 1 refused");
    assert_eq!(first.rule, "continuation-marker-refused");
    assert!(first.message.contains("the next footnote definition is a sibling"));
}

#[test]
fn a_losing_plus_that_meets_its_block_at_the_column_stays_quiet() {
    let source = "[^n]: +\n[^m]: d1\n\n[^m]: +\nflush\n\nsee[^n][^m]\n";
    assert_eq!(
        findings(source),
        [
            "1:7 continuation-marker-refused",
            "4:1 duplicate-footnote-definition",
        ]
    );
}

#[test]
fn a_plus_that_takes_the_block_past_a_losing_plus_stays_quiet() {
    let source = "[^a]: +\n[^a]: +\npara\n\nsee[^a]\n";
    assert_eq!(
        findings(source),
        [
            "2:1 duplicate-footnote-definition",
            "2:7 continuation-marker-refused",
        ]
    );
    assert!(to_html(source).contains("<p>para<a href=\"#fnref1\""));
}

#[test]
fn a_tab_led_losing_plus_is_text() {
    let source = "[^n]: x\n[^n]: \t+\n flush\n\nsee[^n]\n";
    assert_eq!(findings(source), ["2:1 duplicate-footnote-definition"]);
}
