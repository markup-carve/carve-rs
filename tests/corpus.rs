//! Spec-corpus integration tests.
//!
//! Walks `tests/spec/tests/corpus/` (a git submodule of
//! `markup-carve/carve`), pairing every `NN-slug.crv` with its
//! `NN-slug.html` and asserting that `carve::to_html` produces
//! byte-identical output after trimming.
//!
//! Pairs listed in `IMPLEMENTED` are checked. Everything else is
//! emitted as an ignored test so missing constructs stay visible.
//! Promote a slug into `IMPLEMENTED` once the parser + renderer
//! support it.

use std::fs;
use std::path::PathBuf;

/// Corpus pairs the MVP parser + renderer can produce byte-identical
/// HTML for. Grows with each PR.
const IMPLEMENTED: &[&str] = &[
    "01-emphasis",
    "02-headings",
    "03-links",
    "04-images",
    "05-lists",
    "06-task-lists",
    "11-fenced-code",
    "12-inline-code",
];

fn corpus_dir() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest.join("tests/spec/tests/corpus")
}

fn corpus_pairs() -> Vec<String> {
    let dir = corpus_dir();
    if !dir.exists() {
        panic!(
            "Spec corpus not found at {}.\n\
             Did you initialize the submodule?\n  git submodule update --init",
            dir.display()
        );
    }
    let mut out: Vec<String> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()))
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("crv") {
                return None;
            }
            path.file_stem().and_then(|s| s.to_str()).map(String::from)
        })
        .collect();
    out.sort();
    out
}

fn check_pair(slug: &str) {
    let dir = corpus_dir();
    let crv = dir.join(format!("{slug}.crv"));
    let html = dir.join(format!("{slug}.html"));
    let source = fs::read_to_string(&crv).unwrap_or_else(|e| panic!("read {}: {e}", crv.display()));
    let expected =
        fs::read_to_string(&html).unwrap_or_else(|e| panic!("read {}: {e}", html.display()));
    let actual = carve::to_html(&source);
    pretty_assert_eq(slug, expected.trim(), actual.trim());
}

fn pretty_assert_eq(slug: &str, expected: &str, actual: &str) {
    if expected == actual {
        return;
    }
    panic!(
        "corpus pair `{slug}` did not match.\n\n\
         ----- expected -----\n{expected}\n\
         ----- actual -------\n{actual}\n\
         --------------------\n",
    );
}

#[test]
fn corpus_pairs_present() {
    assert!(!corpus_pairs().is_empty(), "no .crv files found in corpus");
}

#[test]
fn all_implemented_pairs_exist() {
    let pairs = corpus_pairs();
    for slug in IMPLEMENTED {
        assert!(
            pairs.iter().any(|p| p == slug),
            "IMPLEMENTED references `{slug}` but no such corpus pair exists. \
             Either the slug is wrong or the submodule is out of date."
        );
    }
}

// One generated test function per implemented slug — keeps panics
// scoped so failures point at the responsible pair.
macro_rules! corpus_test {
    ($name:ident, $slug:literal) => {
        #[test]
        fn $name() {
            check_pair($slug);
        }
    };
}

corpus_test!(c01_emphasis, "01-emphasis");
corpus_test!(c02_headings, "02-headings");
corpus_test!(c03_links, "03-links");
corpus_test!(c04_images, "04-images");
corpus_test!(c05_lists, "05-lists");
corpus_test!(c06_task_lists, "06-task-lists");
corpus_test!(c11_fenced_code, "11-fenced-code");
corpus_test!(c12_inline_code, "12-inline-code");

// Pairs the MVP does not yet handle. Marked `#[ignore]` so they stay
// visible (`cargo test -- --include-ignored`) but don't fail CI.
macro_rules! corpus_todo {
    ($name:ident, $slug:literal) => {
        #[test]
        #[ignore = "not yet implemented in the MVP parser/renderer"]
        fn $name() {
            check_pair($slug);
        }
    };
}

corpus_todo!(
    c07_blockquote_with_attribution,
    "07-blockquote-with-attribution"
);
corpus_todo!(c08_image_with_caption, "08-image-with-caption");
corpus_todo!(c09_tables, "09-tables");
corpus_todo!(
    c10_tables_with_rowspan_and_colspan,
    "10-tables-with-rowspan-and-colspan"
);
corpus_todo!(c13_admonitions, "13-admonitions");
corpus_todo!(c14_abbreviations, "14-abbreviations");
corpus_todo!(c15_mentions_and_tags, "15-mentions-and-tags");
corpus_todo!(c16_inline_extensions, "16-inline-extensions");
corpus_todo!(c17_attributes, "17-attributes");
corpus_todo!(c18_frontmatter, "18-frontmatter");
