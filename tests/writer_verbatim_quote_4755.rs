use carve::api::{Carve, WriteOptions};

#[test]
fn verbatim_reference_keeps_label_quote_marker() {
    let carve = Carve::new();
    let options = WriteOptions::default();
    for source in ["> [][\n> >]\n", "> > [][\n> > >]\n"] {
        assert_eq!(carve.format(source, &options).unwrap(), source);
    }
}
