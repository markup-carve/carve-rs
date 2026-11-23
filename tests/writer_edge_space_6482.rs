//! carve#6482 (ruled A): C2-W-069 holds in every repair round, so the braced form of an emphasis or strong is
//! decided from its content before the rounds. The expected bytes are carve-php's.

use carve::api::{Carve, Options, WriteOptions};

fn check(source: &str, expected: &str) {
    let carve = Carve::new();
    let options = WriteOptions::default();
    let written = carve.format(source, &options).unwrap();
    assert_eq!(written, expected, "{source:?}");
    let tree = carve.parse(source, &Options::default()).unwrap();
    assert_eq!(
        carve.write(&tree, &options).unwrap(),
        expected,
        "{source:?}"
    );
    assert_eq!(carve.format(&written, &options).unwrap(), written);
}

#[test]
fn a_closer_behind_a_line_break_stays_braced_in_the_kind_rounds() {
    check("{*\\`\n*}~\\`*`*\n", "{*\\`\n*}~\\`*\\`*\n");
}

#[test]
fn whitespace_at_a_content_edge_braces_the_node() {
    check(
        "x {* a*} {*b *} {/\u{a0}c/}\n",
        "x {* a*} {*b *} {/\u{a0}c/}\n",
    );
    check("{*\tword\t*}c.\n", "{*\tword\t*}c.\n");
    check("{/\\ a/} {*b\\ *}\n", "{/\\ a/} {*b\\ *}\n");
}

#[test]
fn a_pair_beside_braced_ones_stays_bare_where_it_pairs() {
    check(
        "two words {*é {/\u{a0}x y /} e-f*} and {*\tword\t*}c.\n",
        "two words *é {/\u{a0}x y /} e-f* and {*\tword\t*}c.\n",
    );
    check("{*a {/\tx y/} a, b*}\n", "*a {/\tx y/} a, b*\n");
    check("{/ a/}{*x{/g/}*}\n", "{/ a/}*x{/g/}*\n");
}

#[test]
fn a_content_mark_that_could_pair_braces_the_node() {
    check("/{/e/ x\n", "{/\\{/e/} x\n");
    check("{/x/y/}\n", "{/x/y/}\n");
}

#[test]
fn many_edge_space_pairs_in_one_paragraph() {
    let unit = "{*{  *}{{*x*} {/{ /}{{/x/} {*{ *} {{*{x*} a *b* ";
    let out = "{*{  *}\\{*x* {/{ /}\\{/x/ {*{ *} \\{*{x* a *b*";
    let source = format!("{}\n", unit.repeat(40).trim_end());
    let expected = format!("{}\n", [out; 40].join(" "));
    check(&source, &expected);
}
