//! carve-rs#4792: link text holding escaped closing brackets after a lone `[` writes back as the source.

use carve::api::{Carve, ImportFormat, ImportOptions, LossOptions, Options, WriteOptions};

#[test]
fn link_text_closers_after_a_lone_bracket_format_unchanged() {
    let carve = Carve::new();
    let opts = WriteOptions::default();
    for source in [
        "\\[a \\[[b\\]\\] c](x)\n",
        "*x [b\\]](u)*\n",
        "/\\[x [b\\] c](u)/\n",
        "*\\[[b\\]](u)*\n",
        "> \\[a \\[[b\\]\\] c](x)\n",
        "- *x [b\\]](u)*\n",
    ] {
        let written = carve.format_with_diagnostics(source, &opts).unwrap();
        assert_eq!(written.output, source);
        assert!(
            written.diagnostics.is_empty(),
            "{source:?}: {:?}",
            written.diagnostics
        );
        let tree = carve.parse(source, &Options::default()).unwrap();
        let supplied = carve.write_with_diagnostics(&tree, &opts).unwrap();
        assert!(
            supplied.diagnostics.is_empty(),
            "{source:?}: {:?}",
            supplied.diagnostics
        );
        let again = carve.parse(&supplied.output, &Options::default()).unwrap();
        assert_eq!(
            carve.render_tree(&again, &Options::default()).unwrap(),
            carve.render_tree(&tree, &Options::default()).unwrap(),
            "{source:?} wrote {:?}",
            supplied.output
        );
    }
}

#[test]
fn html_import_keeps_the_link() {
    let carve = Carve::new();
    let imported = carve
        .import_from(
            "<p>[a [<a href=\"x\">b]] c</a></p>\n",
            ImportFormat::Html,
            &ImportOptions::default(),
            &LossOptions::default(),
        )
        .expect("the import succeeds");
    let written = carve
        .write_with_diagnostics(&imported.tree, &WriteOptions::default())
        .expect("the imported tree writes");
    assert_eq!(written.output, "\\[a \\[[b\\]\\] c](x)\n");
    assert!(written.diagnostics.is_empty(), "{:?}", written.diagnostics);
}
