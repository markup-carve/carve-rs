//! A paragraph line below a `%%` line or abbreviation definition erased from
//! inside the paragraph (carve#5012) is written at its authored column, as
//! corpus row 506-5 pins and carve-js writes it (carve-rs#3524).

fn unchanged(source: &str) {
    let written = carve::to_carve(source);
    assert_eq!(written, source, "fmt of {source:?}");
    assert_eq!(carve::to_html(&written), carve::to_html(source));
}

#[test]
fn a_lazy_line_below_two_comments_stays_lazy() {
    unchanged("- intro\n%% a\n  %% b\ntail\n");
}

#[test]
fn a_lazy_line_below_one_comment_stays_lazy() {
    unchanged("- intro\n%% a\ntail\n");
    unchanged("1. intro\n  %% a\ntail\n");
    unchanged("- - intro\n  %% a\ntail\n");
    unchanged("- [ ] intro\n%%\ntail\n");
}

#[test]
fn every_line_below_keeps_its_own_column() {
    unchanged("- intro\n%% a\ntail\n  more\n");
    unchanged("- intro\n%% a\n tail\nmore\n");
    unchanged("- intro\n%% a\n  tail\n%% c\nend\n");
}

#[test]
fn a_line_at_the_content_column_is_unchanged() {
    unchanged("- intro\n%% a\n  tail\n");
}

#[test]
fn a_quoted_paragraph_keeps_its_quote_prefix() {
    unchanged("> intro\n%% a\n> tail\n");
}
