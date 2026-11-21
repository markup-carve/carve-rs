//! Content columns of the data rules' owners: an attribute line above a footnote
//! and a spaced brace block after an item marker leave the column where the
//! marker puts it (C2-B-027, C2-B-090, C2-B-101).

use carve::lint::{LintOptions, lint};

fn found(source: &str, code: &str) -> Vec<(u32, u32)> {
    lint(source, &LintOptions::default())
        .into_iter()
        .filter(|finding| finding.code == code)
        .filter_map(|finding| finding.pos.map(|pos| (pos.start_offset, pos.end_offset)))
        .collect()
}

#[test]
fn attributes_leave_the_content_column_at_the_marker() {
    // This integration test has one test process, so the internal switch is isolated.
    unsafe { std::env::set_var("CARVE_LINT_RULES", "data") };
    let overindented = "block-opener-overindented";
    assert_eq!(found("{#id}\n[^f]: :: term\n", overindented), []);
    assert_eq!(found("{#id .c k=v}\n[^f]: | A | B |\n", overindented), []);
    assert_eq!(
        found("{#id}\n{.a}\n[^f]:   x\n     ::: d\n", overindented),
        []
    );
    assert_eq!(
        found("{#id}\n[^f]: x\n\n         ::: d\n", overindented),
        [(24, 27)]
    );

    let below = "list-item-body-below-content-column";
    assert_eq!(found("1. {.c}\n   c\n   %%%\n", below), []);
    assert_eq!(found("* {.c}\n\n  +\n", below), []);
    assert_eq!(found("1. {.c}\n\n      |\n", below), []);
    assert_eq!(found("1.{.c} t\n\n  x\n", below), [(12, 13)]);
}
