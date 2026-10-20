//! Autolink shapes in a table cell or paragraph that are unambiguous in
//! CommonMark plus GFM are assessed completely, as carve-js and carve-php do
//! (markup-carve/carve#3413).

use carve::migrate_markdown;

fn codes(source: &str) -> Vec<String> {
    migrate_markdown(source)
        .report
        .diagnostics
        .iter()
        .map(|row| row.code.clone())
        .collect()
}

#[test]
fn each_shape_is_assessed_without_a_fallback() {
    for (source, code) in [
        (
            "| <https://example.com/a\\\\\\|b> | z |\n|---|---|\n",
            "markdown-autolink",
        ),
        ("| <mailto:a@b.co> | z |\n|---|---|\n", "markdown-autolink"),
        ("x <mailto:a@b.co> y\n", "markdown-autolink"),
        (
            "| https://e.com/a\\|b | z |\n|---|---|\n",
            "markdown-bare-autolink",
        ),
        (
            "| <https://e.com/a b\\|c> | z |\n|---|---|\n",
            "markdown-bare-autolink",
        ),
        ("x <https://e.com/a b> y\n", "markdown-bare-autolink"),
    ] {
        let found = codes(source);
        assert!(
            !found.iter().any(|c| c == "fidelity-unverified"),
            "{source:?}: {found:?}"
        );
        assert!(found.iter().any(|c| c == code), "{source:?}: {found:?}");
    }
}
