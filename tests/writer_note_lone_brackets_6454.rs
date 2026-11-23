//! C2-W-125 case five: a bracket without a partner in the content of a note or span (carve#6454).
use carve::api::{Carve, Options, WriteOptions};

#[test]
fn lone_brackets_in_a_note_or_span_take_the_backslash() {
    let cases = [
        "^[*\\[*]]]\n",
        "^[[{^]\\]\\]^}]\n",
        "^[*\\[*]]\n",
        "^[[{^]\\]\\]^}]]\n",
        "^[\\]]\\]a\n",
        "*x ^[\\[]a*\n",
        "- *]a^[a\\[a\\[]*\n",
        "> *x ^[aa\\[]*\n",
        "[\\]\\]]{.c}\n",
        "- x [\\]\\]a]{.c}[[\n",
        "> [\\[//a\\[/a/]{.c}b\n",
        "[\\[a]{.c}\n",
    ];
    let carve = Carve::new();
    let options = WriteOptions::default();
    for input in cases {
        assert_eq!(carve.format(input, &options).unwrap(), input, "{input:?}");
        let tree = carve.parse(input, &Options::default()).unwrap();
        assert_eq!(carve.write(&tree, &options).unwrap(), input, "{input:?}");
    }
}
