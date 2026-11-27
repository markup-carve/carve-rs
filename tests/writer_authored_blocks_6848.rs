//! carve#6813, carve#6848: the authored-block fallback of C2-W-088 through the write API with a sidecar, with and
//! without `normalize` (C2-W-127).

use carve::api::{self, Carve, WriteOptions};
use carve::options::Options;

fn refused_paragraph() -> String {
    "[a *[a][]*[x]: /u\n".repeat(122)
}

fn write(src: &str, normalize: bool) -> String {
    let parsed = api::parse_with_layout(src, &Options::default()).unwrap();
    let opts = WriteOptions {
        layout: Some(parsed.layout),
        normalize,
        ..WriteOptions::default()
    };
    Carve::new().write(&parsed.tree, &opts).unwrap()
}

#[test]
fn writes_the_paragraph_as_authored_from_the_sidecar() {
    let src = refused_paragraph();
    assert_eq!(write(&src, false), src);
}

#[test]
fn normalize_still_reads_the_sidecar_source() {
    let src = refused_paragraph();
    assert_eq!(write(&src, true), src);
}

#[test]
fn a_heading_that_reads_back_stays_canonical() {
    let src = format!("# h\n\n{}", refused_paragraph());
    assert_eq!(
        write(&src, false),
        format!("= h\n\n{}", refused_paragraph())
    );
}

#[test]
fn a_tree_without_a_sidecar_keeps_the_loss() {
    let parsed = api::parse_with_layout(&refused_paragraph(), &Options::default()).unwrap();
    assert!(
        Carve::new()
            .write(&parsed.tree, &WriteOptions::default())
            .is_err()
    );
}
