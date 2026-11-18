//! Calls every public function of `carve2::api` once, from outside the crate.

use carve2::api::{self, Carve, EncodeOptions, Error, LintOptions, Options, WriteOptions};

fn main() -> Result<(), Error> {
    let o = Options::default();
    let html = api::render("Hi *there*", &o)?;
    assert_eq!(html, "<p>Hi <strong>there</strong></p>");
    let rendered = api::render_with_diagnostics("x", &o)?;
    assert_eq!(rendered.html, "<p>x</p>");
    let tree = api::parse("# T\n\nx", &o)?;
    let parsed = api::parse_with_diagnostics("x", &o)?;
    assert_eq!(parsed.suppressed_diagnostics, 0);
    assert_eq!(api::render_tree(&tree, &o)?, api::render("# T\n\nx", &o)?);
    api::render_tree_with_diagnostics(&tree, &o)?;
    let json = api::encode_tree(&tree);
    let bare = api::encode_tree_with(&tree, EncodeOptions { positions: false });
    assert!(!bare.contains("\"pos\""));
    let back = api::decode_tree(&api::encode_envelope(&tree))?;
    api::validate_tree(&back)?;
    assert!(api::decode_tree(&json).is_ok());
    assert!(api::include_dependencies(&tree).is_empty());
    api::lint("# A\n\n# A\n", &LintOptions::default())?;
    let carve = Carve::new();
    let w = WriteOptions::default();
    assert_eq!(
        carve.write_to("html", &tree, &w)?.output,
        api::render("# T\n\nx", &o)?
    );
    for refused in [
        api::parse_with_layout("x", &o).map(|_| ()),
        api::import_document("<p>x</p>", &o).map(|_| ()),
        api::migrate_tree(&serde_json::Value::Null).map(|_| ()),
        carve.write(&tree, &w).map(|_| ()),
        carve.format("x", &w).map(|_| ()),
    ] {
        assert!(matches!(refused, Err(Error::Unsupported { .. })));
    }
    let response = api::json::call(r#"{"call":"render","source":"x"}"#);
    assert_eq!(response, r#"{"ok":{"html":"<p>x</p>"}}"#);
    assert_eq!(api::json::status(&response), 0);
    println!("consumer smoke: ok");
    Ok(())
}
