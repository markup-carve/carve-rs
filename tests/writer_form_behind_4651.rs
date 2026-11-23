//! carve-rs#4651: a braced form a repair round gave a pair while an earlier pair read on over it is taken out
//! once that earlier pair takes its own braced form (C2-W-069, C2-W-125). The expected bytes are carve-js's.

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
fn a_strong_behind_a_braced_strong_drops_the_braced_form_of_an_earlier_round() {
    check("> - {*{  *}{{*x*}\n", "> - {*{  *}\\{*x*\n");
    check("{*{  *}{{*x*}\n", "{*{  *}\\{*x*\n");
    check("{*{ *}{{*x*}\n", "{*{ *}\\{*x*\n");
    check(
        "{*{  *}{{*x*} {*{  *}{{*x*} {*{  *}{{*x*}\n",
        "{*{  *}\\{*x* {*{  *}\\{*x* {*{  *}\\{*x*\n",
    );
}

#[test]
fn a_caption_and_an_attribution_drop_it_too() {
    check("> q\n^ {*{  *}{{*x*}\n", "> q\n^ {*{  *}\\{*x*\n");
    check("![a](x.png)\n^ {*{  *}{{*x*}\n", "![a](x.png)\n^ {*{  *}\\{*x*\n");
}

#[test]
fn a_pair_whose_bare_form_cannot_close_keeps_its_braced_form() {
    check(
        "{/{ /}\\{{/x /} {/{/} {{*/*}{{\n",
        "{/{ /}{{/x /} {/{/} \\{*/*{{\n",
    );
}
