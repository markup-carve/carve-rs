//! Djot import of carve#6693: an image whose reference is not defined is `<img>` with no alt and no src,
//! as djot.js writes it.

use carve::api::{Carve, ImportFormat, ImportOptions, LossOptions, WriteOptions};

fn import(source: &str) -> String {
    let carve = Carve::new();
    let imported = carve
        .import_from(
            source,
            ImportFormat::Djot,
            &ImportOptions::default(),
            &LossOptions::default(),
        )
        .expect("the import succeeds");
    assert!(
        imported
            .diagnostics
            .iter()
            .all(|e| e.channel != "render-loss"),
        "{source:?}: {:?}",
        imported.diagnostics
    );
    carve
        .write_with_diagnostics(&imported.tree, &WriteOptions::default())
        .expect("the imported tree writes")
        .output
}

#[test]
fn an_image_with_an_undefined_reference_has_no_alt_and_no_src() {
    let cases = [
        ("![a][r]\n", "\n"),
        ("x ![a][r] y\n", "x  y\n"),
        ("![a *b*][]\n", "\n"),
        ("![a][r]\n\n[r]: /u\n", "![a](/u)\n"),
        ("![a][]\n\n[a]: /u\n", "![a](/u)\n"),
        ("![a](r)\n", "![a](r)\n"),
    ];
    for (source, expected) in cases {
        assert_eq!(import(source), expected, "{source:?}");
    }
}
