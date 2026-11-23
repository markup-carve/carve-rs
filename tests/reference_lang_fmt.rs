//! C2-W-003: a reference whose brace block holds only a language is written with one brace block.
use carve::api::{Carve, WriteOptions};

#[test]
fn reference_with_only_a_language_formats_stably() {
    let carve = Carve::new();
    let opts = WriteOptions::default();
    for source in [
        "[x][a]{:own}\n\n[a]: /u\n",
        "![x][a]{:own}\n\n[a]: /u\n",
        "[a][]{:own}\n\n[a]: /u\n",
        "[x][a]{:own}\n\n{#d .p .c k=1}\n[a]: /u\n",
    ] {
        assert_eq!(carve.format(source, &opts).unwrap(), source);
    }
}
