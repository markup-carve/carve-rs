//! The default lint mode reads an attribute-line candidate after a container marker as the
//! `block-marker-as-text` row of the lint model does, and reports every unattached run of a text.

use carve::lint::{LintOptions, lint};

fn found(source: &str, code: &str) -> Vec<(usize, usize)> {
    lint(source, &LintOptions::default())
        .into_iter()
        .filter(|finding| finding.code == code)
        .filter_map(|finding| finding.pos.map(|pos| (pos.start_offset, pos.end_offset)))
        .collect()
}

#[test]
fn a_failed_attribute_line_after_a_container_marker_is_reported() {
    for host in ["", "> ", "- ", "1. ", "a\n", "> a\n> ", ":: t\n: ", "> > "] {
        for shape in ["{#id", "{.x", "{_x", "{key=v", "{_x_ y", "{a="] {
            let source = format!("{host}{shape} {{_y_}}\n");
            let at = source.rfind(shape).unwrap();
            let end = source.trim_end().len();
            assert_eq!(
                found(&source, "block-marker-as-text"),
                [(at, end)],
                "{source:?}"
            );
        }
    }
}

#[test]
fn carve_rs_4573_reproductions() {
    assert_eq!(found("> {_x\n", "block-marker-as-text"), [(2, 5)]);
    assert_eq!(found("- {.x y\n", "block-marker-as-text"), [(2, 7)]);
    assert_eq!(found(":: t\n: {_\nmore\n", "block-marker-as-text"), [(7, 9)]);
    assert_eq!(found("> {_e_} x\n", "block-marker-as-text"), []);
    assert_eq!(found("> {.c}\n", "block-marker-as-text"), []);
}

#[test]
fn a_caption_paragraph_is_read_from_the_caption_line() {
    assert_eq!(found("> {+x+}tail\n^ {.x\n", "block-marker-as-text"), []);
}

#[test]
fn every_unattached_run_of_a_text_is_reported() {
    let code = "unattached-inline-attribute";
    assert_eq!(found("a\n> {.c} {.c}\n", code), [(4, 8), (9, 13)]);
    assert_eq!(found("x\n> > {_x {.c}\n", code), [(10, 14)]);
    assert_eq!(found("a {.c} b {.c}\n", code), [(2, 6), (9, 13)]);
    assert_eq!(found("a {.c}{#d} b\n", code), [(2, 10)]);
}

#[test]
fn a_line_block_line_is_no_host_of_the_rule() {
    let code = "unattached-inline-attribute";
    assert_eq!(found("::: |\na {.c} b\n:::\n", code), []);
    assert_eq!(found("::: |\n{~x~} {.c}\n:::\n", code), []);
}
