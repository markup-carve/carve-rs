# Shared migration results

`migrate_html`, `migrate_markdown`, `migrate_djot`, and `migrate_bbcode` return the same
`MigrationResult` shape. This lets applications build one import workflow
instead of special-casing HTML reports and treating other formats as strings.
`try_migrate_markdown` returns the same result inside `Result`, with a writer
error if the imported document cannot be spelled as Carve.

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
construct-specific diagnostics. 

Markdown, Djot, and BBCode verify a narrow literal-text subset: empty input or
Unicode letters and numbers separated by single ASCII spaces, with optional
trailing line endings. When the imported text matches and no known loss was
reported, `literal-text-verified` records preserved/exact evidence. All other
inputs retain the dropped/fallback `fidelity-unverified` warning.

Markdown reports each leading `---` block it converts to frontmatter as
`frontmatter-synthesized`, info/normalized at `line:1`: the content survives
byte-exact, and the reading of the `---` run was decided - an alternate block
form resolved. Confidence follows the opener, `inferred` for a bare `---` and
`exact` for a typed one. It also reports each blank GFM table row it drops as
`structure-unspellable`. This deliberately fails closed at the worst-case
outcome: byte differences are not evidence of semantic fidelity.

Markdown reports `raw-code-fallback` with warning severity, degraded fidelity
and exact confidence when an HTML code payload needs raw HTML to preserve its
content or structure in Carve source. The HTML output keeps the code payload;
targets and profiles that escape or omit raw HTML change its structure and
content. This loss applies to code emitted as raw HTML; native Code output does not
receive it.

Markdown reports `raw-span-whitespace-trimmed`, warning/degraded/exact, at the
source line of every raw span whose content would end a content line in
whitespace. CARVE-P2-025 drops a whitespace run at the end of every content
line, and a verbatim run crossing a line break is no exception, so this input:

```markdown
<a href="foo  
bar">
```

is written with its two spaces and read back without them. Degraded rather than
dropped, because the span and its text survive and the whitespace does not; it
is reported rather than respelled as a raw block, which would keep the bytes at
the cost of a different block structure (markup-carve/carve#2804). Whitespace a
raw span carries anywhere but a line end is not reported, because Carve keeps
it.

Version 2 renames version 1's `Carried` value to `Preserved` and adds
`Normalized`. Consumers should inspect `schema_version` before interpreting
the fidelity enum.

The immediate value is safer migrations, consistent binding APIs, and a stable
place for future source ranges, confidence, safe fixes, batch reports, and
round-trip checks.

The CLI serializes the shared report with `carve migrate --report PATH`
(`--report -` writes it to stderr). HTML's
existing importer API and report remain available unchanged.
