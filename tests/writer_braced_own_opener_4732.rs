use carve::api::{Carve, Options, WriteOptions};

#[test]
fn a_bare_pair_keeps_its_own_unclosed_braced_opener_as_text() {
    let engine = Carve::new();
    let write = WriteOptions::default();
    for source in ["*{*a*\n", "/{/a/\n", "x *{*a* y\n", "*{**\n", "/;//{/{//\n"] {
        let tree = engine.parse(source, &Options::default()).unwrap();
        assert_eq!(engine.write(&tree, &write).unwrap(), source, "{source:?}");
        assert_eq!(engine.format(source, &write).unwrap(), source, "{source:?}");
    }
}
