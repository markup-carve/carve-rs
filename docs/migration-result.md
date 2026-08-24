# Shared migration results

`migrate_html`, `migrate_markdown`, and `migrate_djot` return the same
`MigrationResult` shape. This lets applications build one import workflow
instead of special-casing HTML reports and treating other formats as strings.

```rust
use carve::{migrate_html, HtmlImportOptions, MigrationFidelity};

let result = migrate_html(
    "<p><kbd kbd=lit>text</kbd></p>",
    &HtmlImportOptions::default(),
)?;

for diagnostic in &result.report.diagnostics {
    if diagnostic.fidelity == MigrationFidelity::Dropped {
        eprintln!("{}: {}", diagnostic.code, diagnostic.message);
    }
}
std::fs::write("document.crv", result.value)?;
# Ok::<(), carve::HtmlImportError>(())
```

Markdown and Djot currently return explicit empty reports. That is useful even
before those importers gain detailed diagnostics: consumers can depend on one
versioned envelope and add policy without changing their control flow later.

The immediate value is safer migrations, consistent binding APIs, and a stable
place for future source ranges, confidence, safe fixes, batch reports, and
round-trip checks.

This draft does not yet serialize the shared report through the CLI. HTML's
existing importer API and report remain available unchanged.
