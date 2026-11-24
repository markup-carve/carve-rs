use carve::api::{Carve, Options, WriteOptions};

#[test]
fn code_openers_repaired_by_kind_include_the_span_text() {
    let carve = Carve::new();
    let options = WriteOptions::default();
    for (source, expected) in [
        ("[\\` ]{}`\n", "[\\` ]{}\\`\n"),
        ("[\\`]{}`\n", "[\\`]{}\\`\n"),
        ("[\\` ]{.c}`\n", "[\\` ]{.c}\\`\n"),
    ] {
        let tree = carve.parse(source, &Options::default()).unwrap();
        assert_eq!(carve.write(&tree, &options).unwrap(), expected);
        assert_eq!(carve.format(expected, &options).unwrap(), expected);
    }
}
