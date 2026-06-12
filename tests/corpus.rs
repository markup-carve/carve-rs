//! Spec-corpus integration tests.
//!
//! Walks `tests/spec/tests/corpus/` (a git submodule of
//! `markup-carve/carve`), pairing every `NN-slug.crv` with its
//! `NN-slug.html` and asserting that `carve::to_html` produces
//! byte-identical output after trimming.
//!
//! Every pair in an implemented category is checked by
//! `all_implemented_corpus_pairs_match`; named tests keep representative
//! failures scoped.

use std::fs;
use std::path::PathBuf;

const IMPLEMENTED: &[&str] = &[
    "01-emphasis",
    "02-headings",
    "03-links",
    "04-images",
    "05-lists",
    "06-task-lists",
    "07-blockquote-with-attribution",
    "08-image-with-caption",
    "09-tables",
    "10-tables-with-rowspan-and-colspan",
    "11-fenced-code",
    "12-inline-code",
    "13-admonitions",
    "14-abbreviations",
    "15-mentions-and-tags",
    "16-inline-extensions",
    "17-attributes",
    "18-frontmatter",
    "34-reference-link",
    "35-collapsed-reference-link",
    "42-math",
    "43-footnotes",
    "84-block-attribute-lines",
    "85-numbered-cross-references",
    "86-inline-footnotes",
    "87-list-item-attributes",
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

fn is_implemented_pair(slug: &str) -> bool {
    IMPLEMENTED.iter().any(|implemented| {
        slug == *implemented
            || slug.strip_prefix(implemented).is_some_and(|rest| {
                rest.starts_with('-') && rest[1..].bytes().all(|b| b.is_ascii_digit())
            })
    })
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

#[test]
fn all_implemented_corpus_pairs_match() {
    for slug in corpus_pairs() {
        if is_implemented_pair(&slug) {
            check_pair(&slug);
        }
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
corpus_test!(
    c07_blockquote_with_attribution,
    "07-blockquote-with-attribution"
);
corpus_test!(c08_image_with_caption, "08-image-with-caption");
corpus_test!(c09_tables, "09-tables");
corpus_test!(
    c10_tables_with_rowspan_and_colspan,
    "10-tables-with-rowspan-and-colspan"
);
corpus_test!(c11_fenced_code, "11-fenced-code");
corpus_test!(c12_inline_code, "12-inline-code");
corpus_test!(c13_admonitions, "13-admonitions");
corpus_test!(c14_abbreviations, "14-abbreviations");
corpus_test!(c15_mentions_and_tags, "15-mentions-and-tags");
corpus_test!(c16_inline_extensions, "16-inline-extensions");
corpus_test!(c17_attributes, "17-attributes");
corpus_test!(c18_frontmatter, "18-frontmatter");
corpus_test!(
    c85_numbered_cross_references,
    "85-numbered-cross-references"
);
corpus_test!(c86_inline_footnotes, "86-inline-footnotes");
