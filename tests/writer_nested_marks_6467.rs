//! carve#6467 and carve#6468: the C2-W-125 repairs of nested braced marks place only the backslash the
//! procedure of C2-W-088 places. The expected bytes are those of the carve-js writer-gate reference.

use carve::api::{Carve, Options, WriteOptions};

fn check(source: &str, expected: &str) {
    let carve = Carve::new();
    let options = WriteOptions::default();
    let written = carve.format(source, &options).unwrap();
    assert_eq!(written, expected, "{source:?}");
    let tree = carve.parse(source, &Options::default()).unwrap();
    assert_eq!(carve.write(&tree, &options).unwrap(), expected, "{source:?}");
    assert_eq!(carve.format(&written, &options).unwrap(), written);
}

#[test]
fn a_brace_of_case_one_takes_out_the_earlier_repair_of_its_text() {
    check("{,{,,\\{,},,},}\n", "{,{,,\\{,},,},}\n");
    check("{,{,,\\{,} ,},}\n", "{,{,,\\{,} ,},}\n");
    check("{^{^^\\{^}^^}^}\n", "{^{^^\\{^}^^}^}\n");
}

#[test]
fn a_brace_that_reads_on_past_the_content_takes_the_backslash_before_the_closer() {
    check("- {,{,,{\\,,},,},}\n", "- {,{,,\\{,,},,},}\n");
    check("{,{,,{\\, ,},,},}\n", "{,{,,\\{, ,},,},}\n");
}

#[test]
fn a_brace_at_the_end_of_a_text_takes_the_kind_of_the_mark_behind_it() {
    check("{ ,{,,\\{,},,},}\n", "{ ,{,,\\{,},,},}\n");
    check("},{,,\\{,},,},}\n", "},{,,\\{,},,},}\n");
    check("{^^^\\{^}^^}^}\n", "{^^^\\{^}^^}^}\n");
}

#[test]
fn a_strong_braced_after_its_rounds_keeps_no_escape_of_the_bare_form() {
    check("- {*{ *} {{x]*}\n", "- {*{ *} {{x]*}\n");
    check("{*{ *} {{x]*}\n", "{*{ *} {{x]*}\n");
    check("- {/{ /} {{x]/}\n", "- {/{ /} {{x]/}\n");
}
