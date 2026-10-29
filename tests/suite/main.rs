//! Every integration test except the few below, compiled as ONE binary.
//!
//! One file per binary meant ~750 crates to compile and link on every CI run.
//! Kept out, because CI addresses them by target name: `perf_regressions`
//! (guards.yml), `includes` and `include_conformance` (the no-fs job).
//!
//! `suite_modules!()` declares every `tests/*.rs` that Cargo.toml does not
//! build as its own `[[test]]` target and that this file does not register by
//! hand, so a new test file needs no line here.

carve_suite_registry::suite_modules!();

#[path = "../common/mod.rs"]
mod common;

mod registry {
    use std::collections::BTreeSet;
    use std::path::Path;

    fn registered(text: &str, prefix: &str) -> BTreeSet<String> {
        text.lines()
            .filter_map(|line| {
                let name = line.trim().strip_prefix(prefix)?.split('"').next()?;
                (!name.contains('/')).then(|| name.to_string())
            })
            .collect()
    }

    /// With `autotests = false`, a file nobody registers is never compiled, so
    /// its tests would pass by not existing. The macro reads the directory when
    /// the suite compiles, and rustc does not track that listing, so this also
    /// catches a build that predates a file added or moved since.
    #[test]
    fn every_test_file_is_compiled() {
        let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
        let on_disk: BTreeSet<String> = std::fs::read_dir(&tests)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .filter(|name| name.ends_with(".rs"))
            .collect();
        let manifest = std::fs::read_to_string(tests.join("../Cargo.toml")).unwrap();
        let own_targets = registered(&manifest, "path = \"tests/");
        let by_hand = registered(include_str!("main.rs"), "#[path = \"../");
        let generated: BTreeSet<String> = super::GENERATED_TEST_FILES
            .iter()
            .map(|name| name.to_string())
            .collect();

        let twice: Vec<_> = generated
            .intersection(&own_targets)
            .chain(generated.intersection(&by_hand))
            .chain(own_targets.intersection(&by_hand))
            .collect();
        let compiled: BTreeSet<String> = generated
            .iter()
            .chain(&own_targets)
            .chain(&by_hand)
            .cloned()
            .collect();
        let missing: Vec<_> = on_disk.difference(&compiled).collect();
        let gone: Vec<_> = compiled.difference(&on_disk).collect();
        assert!(
            twice.is_empty() && missing.is_empty() && gone.is_empty(),
            "this suite build does not match tests/: not compiled {missing:?}, \
             compiled twice {twice:?}, gone {gone:?}. The suite was built before \
             the files changed; `touch tests/suite/main.rs` and run it again"
        );
    }
}

// Native Djot fixtures retain image-only paragraphs from before carve#3709.
fn image_paragraphs_as_images(html: &str) -> String {
    static PARAGRAPH: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    PARAGRAPH
        .get_or_init(|| regex::Regex::new(r"<p(\s[^>]*)?>\s*(<img\b[^>]*>)\s*</p>").unwrap())
        .replace_all(html, |captures: &regex::Captures<'_>| {
            let image = &captures[2];
            match captures.get(1) {
                Some(attrs) => format!("{}{}>", &image[..image.len() - 1], attrs.as_str()),
                None => image.to_owned(),
            }
        })
        .into_owned()
}
