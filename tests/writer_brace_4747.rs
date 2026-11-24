use carve::api::{Carve, Options, WriteOptions};

#[test]
fn braced_pair_after_a_literal_brace_needs_no_eager_escape() {
    let carve = Carve::new();
    let options = WriteOptions::default();
    for source in ["{{*a*}*\n", "x{{*{}\\ a*}*\n"] {
        let tree = carve.parse(source, &Options::default()).unwrap();
        assert_eq!(carve.write(&tree, &options).unwrap(), source);
    }
}

