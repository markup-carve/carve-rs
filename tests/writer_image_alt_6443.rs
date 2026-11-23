//! C2-W-089, C2-W-097: an image alt whose clause spelling does not read back takes a form that does (carve#6443).
use carve::api::{Carve, Options, WriteOptions};

#[test]
fn a_block_image_alt_beside_atoms_is_written_back_unchanged() {
    let cases = [
        // A link tail in the alt is an atom, so the `]` of `[^n]` stays bare.
        "![a [^n][a](b]) b](u \"t\")\n%% c\n",
        // An autolink in the alt is an atom, so the `[` before it has no partner.
        "![\\[`` ` ``<https://e.x/]>](a\\)b)\n%% c\n",
    ];
    let carve = Carve::new();
    let options = WriteOptions::default();
    for input in cases {
        assert_eq!(carve.format(input, &options).unwrap(), input, "{input:?}");
        let tree = carve.parse(input, &Options::default()).unwrap();
        assert_eq!(carve.write(&tree, &options).unwrap(), input, "{input:?}");
    }
}

#[test]
fn an_alt_whose_clause_spelling_misreads_takes_a_later_form() {
    let carve = Carve::new();
    let options = WriteOptions::default();
    for input in [
        "![[^n]`]`\\]](u \"t\")\n",
        "![[^n]<https://e.x/]>\\]](u)\n",
        "![\\[\\][a](b])](u)\n",
        "![a\\[[a](b])](u)\n",
    ] {
        let written = carve.format(input, &options).unwrap();
        assert_eq!(
            carve.render(&written, &Options::default()).unwrap(),
            carve.render(input, &Options::default()).unwrap(),
            "{input:?}"
        );
    }
}
