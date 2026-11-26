use carve::api::{Carve, WriteOptions};

#[test]
fn comment_closer_does_not_create_another_escape_kind() {
    let carve = Carve::new();
    let options = WriteOptions::default();
    for source in ["\\{#\n#}\n{\n", "\\{%\n%}\n}\n"] {
        assert_eq!(carve.format(source, &options).unwrap(), source);
    }
}
