//! carve#6469: under C2-W-069 a strong or emphasis whose content starts or ends with its own mark is written
//! braced, also when a later delimiter of the same kind follows the node. The expected bytes are carve-php's.

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
fn a_content_star_glued_to_the_closer_braces_the_strong() {
    check("*]\\**^*|\n", "{*]**}^*|\n");
    check("*a\\**^*|\n", "{*a**}^*|\n");
    check("x *a\\** y*\n", "x {*a**} y*\n");
    check("*]\\**\n", "{*]**}\n");
}

#[test]
fn a_content_mark_at_either_edge_braces_the_node() {
    check("/]\\//^/|\n", "{/]//}^/|\n");
    check("*\\*a*^*|\n", "{**a*}^*|\n");
    check("*a \\** x*\n", "{*a **} x*\n");
}
