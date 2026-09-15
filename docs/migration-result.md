# Shared migration results

`migrate_html`, `migrate_markdown`, `migrate_djot`, and `migrate_bbcode` return the same
`MigrationResult` shape. This lets applications build one import workflow
instead of special-casing HTML reports and treating other formats as strings.

```rust
use carve::{migrate_html, HtmlImportOptions, MigrationFidelity};

let result = migrate_html(
    "<p><kbd kbd=lit>text</kbd></p>",
    &HtmlImportOptions::default(),
)?;

for diagnostic in &result.report.diagnostics {
    if matches!(diagnostic.fidelity, MigrationFidelity::Degraded | MigrationFidelity::Dropped) {
        eprintln!("{}: {}", diagnostic.code, diagnostic.message);
    }
}
std::fs::write("document.crv", result.value)?;
# Ok::<(), carve::HtmlImportError>(())
```

Version 2 reports use the same `Preserved`, `Normalized`, `Degraded`, and
`Dropped` fidelity vocabulary as the other Carve engines. HTML retains its
construct-specific diagnostics. Markdown, Djot, and BBCode currently emit a
`fidelity-unverified` dropped/fallback warning on every import because those
paths do not yet expose construct-level loss information. This deliberately
fails closed at the worst-case outcome: byte differences are not evidence of
semantic fidelity.

Version 2 renames version 1's `Carried` value to `Preserved` and adds
`Normalized`. Consumers should inspect `schema_version` before interpreting
the fidelity enum.

The immediate value is safer migrations, consistent binding APIs, and a stable
place for future source ranges, confidence, safe fixes, batch reports, and
round-trip checks.

The CLI serializes the shared report with `carve migrate --report PATH`
(`--report -` writes it to stderr). HTML's
existing importer API and report remain available unchanged.
