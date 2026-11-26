use carve::api::{Carve, Options, WriteOptions};

#[test]
fn nested_pair_marks_are_scoped_to_the_inner_node() {
    let carve = Carve::new();
    let options = WriteOptions::default();
    for (source, expected) in [
        ("*x /*a/ y*\n", "*x /*a/ y*\n"),
        ("*/*a/*\n", "*/*a/*\n"),
        ("*{/{-*a/}*\n", "*/{-*a/*\n"),
    ] {
        let tree = carve.parse(source, &Options::default()).unwrap();
        assert_eq!(carve.write(&tree, &options).unwrap(), expected);
        assert_eq!(carve.format(expected, &options).unwrap(), expected);
    }
}
