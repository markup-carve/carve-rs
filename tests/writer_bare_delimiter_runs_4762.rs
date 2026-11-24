//! A repeated bare delimiter remains text even when followed by a closing brace.

use carve::api::{Carve, Options, WriteOptions};

#[test]
fn repeated_bare_marks_before_a_closing_brace() {
    let carve = Carve::new();
    for source in ["/a//}/\n", "/~//}/\n", "*:**};*\n"] {
        let tree = carve.parse(source, &Options::default()).unwrap();
        let written = carve.write(&tree, &WriteOptions::default()).unwrap();
        assert_eq!(written, source);
        assert_eq!(
            carve.format(source, &WriteOptions::default()).unwrap(),
            source
        );
        let read_back = carve.parse(&written, &Options::default()).unwrap();
        assert_eq!(
            carve.write(&read_back, &WriteOptions::default()).unwrap(),
            written
        );
    }
}
