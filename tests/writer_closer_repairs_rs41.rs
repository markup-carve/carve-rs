//! Shapes of the carve#6411 sweep that carve-rs wrote apart from carve-js: carve-rs#4645 (no escape behind a
//! content that ended early), carve#6461 (a closer repair gives no kind), carve#6462 (a strong that reads on
//! takes its braced form) and carve-rs#4646 (the probe across a link's closer). The expected bytes are carve-js's.

use carve::api::{Carve, Options, WriteOptions};

fn check(source: &str, expected: &str) {
    check_paths(source, expected, expected);
}

/// `plain` is the form written from the tree without the sidecar of the parse.
fn check_paths(source: &str, expected: &str, plain: &str) {
    let carve = Carve::new();
    let options = WriteOptions::default();
    let written = carve.format(source, &options).unwrap();
    assert_eq!(written, expected, "{source:?}");
    let tree = carve.parse(source, &Options::default()).unwrap();
    assert_eq!(carve.write(&tree, &options).unwrap(), plain, "{source:?}");
    assert_eq!(carve.format(&written, &options).unwrap(), written);
}

#[test]
fn nothing_behind_a_content_that_ended_early_asks_for_an_escape() {
    check("{*}\\*}*}|\n", "{*}\\*}*}|\n");
    check("*}\\*}*^\n", "{*}\\*}*}^\n");
    check("x(/u)\\/}/}\n", "x({/u)\\/}/}}\n");
}

#[test]
fn a_closer_repair_gives_no_kind_in_the_group_rounds() {
    check("- \\{^{^^{^^}^^}^}\n", "- \\{^{^^\\{^\\^}^^}^}\n");
    // Without the closer kinds a node is lost, so the group is written again with them (C2-W-005).
    check(
        "{^{^^}{,^{^[,}^}{^]\\{}[^}^}\n",
        "{^\\{\\^\\^}{,\\^\\{\\^[,}^}{^]\\{}[^}\\^}\n",
    );
}

#[test]
fn a_strong_that_reads_on_takes_its_braced_form() {
    check("{*{ *}[*[*]\n", "{*{ *}[*[*]\n");
}

#[test]
fn a_construct_across_a_link_closer_escapes_its_opener() {
    // Written from the tree without the sidecar, carve-js gives this form as well.
    check_paths(
        "[*[[[^][]]]()*\n",
        "[\\*\\[[[^][]\\]]()*\n",
        "[*\\[[[^][]\\]]()\\*\n",
    );
    check("[[*{^[ [^]]]()*\n", "[\\[\\*\\{^\\[ \\[^\\]\\]]()*\n");
}
