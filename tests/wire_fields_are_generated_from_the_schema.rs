//! `src/wire_fields.rs` is GENERATED from the pinned AST schema, and PART 12
//! section 11 is only as good as that map: a field the schema gained but the
//! map does not name would be REFUSED on ingest even though it is valid.
//!
//! A committed copy of a file that lives somewhere else rots silently, so the
//! generator runs here and the result is compared.

use std::process::Command;

#[test]
fn the_generated_wire_field_map_matches_the_pinned_schema() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    if !root.join("tests/spec/resources/ast-schema.json").exists() {
        panic!("spec submodule missing; run `git submodule update --init`");
    }

    let output = Command::new("python3")
        .arg(root.join("tools/generate-wire-fields.py"))
        .arg("--check")
        .current_dir(root)
        .output()
        .expect("python3 runs the generator");

    assert!(
        output.status.success(),
        "src/wire_fields.rs is stale - run `python3 tools/generate-wire-fields.py`\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
