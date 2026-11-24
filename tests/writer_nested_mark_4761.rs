use carve::api::{Carve, Options, WriteOptions};

#[test]
fn nested_emphasis_text_can_force_the_outer_strong_to_brace() {
    let carve = Carve::new();
    let options = WriteOptions::default();
    for (source, expected) in [
        ("{*/a*/*}\n", "{*/a*/*}\n"),
        ("*/a\\*/*\n", "{*/a*/*}\n"),
        ("*/a)\\*/*\n", "{*/a)*/*}\n"),
    ] {
        let tree = carve.parse(source, &Options::default()).unwrap();
        assert_eq!(carve.format(source, &options).unwrap(), expected);
        assert_eq!(carve.write(&tree, &options).unwrap(), expected);
    }
}

#[test]
fn a_literal_mark_run_inside_emphasis_does_not_brace_the_strong() {
    let carve = Carve::new();
    let options = WriteOptions::default();
    let source = "*x /a**/ y*\n";
    let tree = carve.parse(source, &Options::default()).unwrap();
    assert_eq!(carve.write(&tree, &options).unwrap(), source);
    assert_eq!(carve.format(source, &options).unwrap(), source);
}
