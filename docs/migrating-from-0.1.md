# Migrating from carve-lang 0.1.14

This page lists the public API of the last 0.1 release, carve-lang 0.1.14, that the [README migration table](../README.md#migrating-from-01) does not name, and what a 0.1 user moves to. The README table covers the calls the bindings use; this page covers the rest of the 0.1 crate root. The full 0.1 reference stays on [docs.rs](https://docs.rs/carve-lang/0.1.14/carve/).

Read the CHANGELOG's mentions of the `carve2` crate as release history: `carve2` never shipped. A 0.1 user moves from carve-lang 0.1.14 straight to carve-lang 0.2, whose library is `carve::api`.

## Same name under `carve::api`

These 0.1 root items keep their names in 0.2 and moved under `carve::api`:

| 0.1 item | 0.2 path |
|---|---|
| `AutolinkOptions`, `CrossrefStyle`, `ExternalLinksOptions`, `HeadingNumbersOptions`, `HeadingReferenceOptions`, `ListType` | `api::extensions` |
| `CitationMode` | `api::extensions`, read by `CitationsOptions` |
| `CodeGroupOptions` | `api::extensions` |
| `ContentMode`, `FencedRenderOptions` | `api::extensions` |
| `QuoteCharacters`, `SMART_QUOTE_LOCALES` | `api::extensions` |
| `SanitizeResult` | `api::extensions`, returned by `sanitize_svg` |
| `SwatchPosition`, `SwatchShape` | `api::extensions` |
| `UrlGenerator`, `WikilinksOptions` | `api::extensions` |
| `ProseMirrorImport` | `api::prosemirror` |

Each options type now goes to a factory in `api::extensions` (`autolink`, `external_links`, `heading_numbers`, `fenced_render`, `wikilinks` and the rest), which returns a `Bundle` for `Carve::register`.

## `carve::profile` is a different module

In 0.1, `carve::profile` was the profile filter: the `Profile` type and the link policy that ran before rendering. In 0.2, `carve::profile` is the stage-timing hook module of the `bench/` timing program. It exists only with the `profile` Cargo feature, which is off by default, and it is no host API. The profile is now the `Options::profile` field (`api::Profile`), and the link policy is the `api::extensions::link_policy` factory.

## Removed root items by area

`gone` means 0.2 has no counterpart.

| 0.1 items | 0.2 |
|---|---|
| `ast_json`, `try_to_json`, `to_json_with_options`, `AstJsonError`, `AstJsonErrorKind`, `AST_CONTRACT_VERSION`, `CORE_AST_VOCABULARY`, `RawNodeType` | `api::encode_tree`, `api::encode_tree_with`, `api::decode_tree` and `api::validate_tree` read and write 2.0 trees; `api::NODE_TYPES` lists the node types. Errors are `api::Error` |
| `ast_envelope`, `to_ast_envelope_json`, `from_ast_envelope_json`, `AstEnvelopeError`, `AstEnvelopeExtension`, `AstEnvelopeOptions`, `AstEnvelopeReaderOptions` | `api::encode_envelope` writes an envelope; `api::decode_tree_with` and `TreeReadOptions` read one |
| `ast_sidecars`, `AstSidecarError`, `AnnotationAnchor`, `AnnotationRange`, `AnnotationRanges`, `to_annotation_ranges_json`, `from_annotation_ranges_json`, `to_source_layout_json` | gone as JSON sidecars. Tree nodes carry their source positions (`api::encode_tree_with` writes them), `api::position_of` turns a byte offset of the source into a line and column, and `api::parse_with_layout` returns the source-layout record |
| `NodeProvenance`, `Provenance`, `ProvenanceOrigin`, `ProvenanceSource`, `authored_provenance`, `parse_with_authored_provenance`, `to_provenance_json`, `from_provenance_json` | gone |
| `fresh_node_identity`, `to_node_identity_json`, `from_node_identity_json` | `api::NodeIdentity` with `api::parse_snapshot_with_identity` |
| `ast_patch`, `AstPatchError`, `AstPatchOperation` | `api::diff_tree` and `api::apply_patch` with 0.2 patches |
| `ReversibleAstPatch` | gone: 0.2 has no reversible patch |
| `SourcePatch`, `SourcePatchError`, `SourceEdit`, `SourceEditKind`, `SourceSuggestion`, `SourceUnspellable`, `create_source_patch`, `apply_source_patch`, `to_carve_patch` | gone: no source patches in 0.2. Edit the tree and write it back with `Carve::write` |
| `ast_merge`, `MergeConflict`, `MergeConflictReason`, `MergeResolution`, `MergeResult` | gone: no merge API in 0.2 |
| `render_html_owned`, `render_html_owned_with_options`, `to_html_with_options_and_report`, `RenderResult`, `RenderLoss`, `RenderLossError`, `RenderDepthError`, `CheckedRenderOptions`, `DEFAULT_MAX_RENDER_LOSSES`, `MAX_RENDER_DEPTH`, `expands_for_target` | `api::render_tree`, or `api::render_tree_with_diagnostics` for the losses (`Rendered { html, diagnostics, suppressed_diagnostics }`). `Options::max_losses`, `allow_losses` and `strict_losses` replace the loss settings |
| `RenderTarget`, `render_carve_with_options`, `to_carve_with_options`, `CarveWriterOptions`, `RenderCarveError`, `to_markdown_with_options`, `MarkdownRenderError`, `to_plain_text_with_options`, `to_ansi_with_options` | `Carve::write` with `WriteOptions`, or a writer from `Writers`; the targets are `carve`, `html`, `markdown`, `plain` and `ansi`. The exports also live in `carve::export` |
| `SmartTypographyMode` | `Options::smart_typography` |
| `label_default`, `LABEL_FOOTNOTE_BACKLINK` | `Options::labels` |
| `apply_profile`, `profile_filter`, `ProfileFilterResult`, `ProfileViolation`, `ProfileViolationError`, `DisallowedAction` | `Options::profile` on `api::parse` and `api::render`. 0.2 has no standalone filter call |
| `LinkPolicy` | `api::extensions::link_policy(LinkPolicyOptions)` |
| `prepare_doc_with_includes`, `PreparedWithIncludes`, `IncludeResult` | `Options::includes` on `api::parse`: the tree it returns is resolved |
| `IncludeResolver`, `FileSystemResolver` | `api::Resolver`; the `fs` feature's resolver over a root directory is `carve::include::fs::resolver` |
| `IncludeDependency` | `api::Dependency` from `api::include_dependencies` |
| `IncludeWarning`, `IncludeDenial` | diagnostics (`api::Entry`) of `parse_with_diagnostics` and `render_with_diagnostics` |
| `IncludeRegion`, `IncludeRegionError`, `IncludeRegionMode` | `include::region_lines`, see the README table |
| `DEFAULT_MAX_DEPTH`, `DEFAULT_MAX_FILE_BYTES` | `Includes::depth` and `Includes::bytes`: a host may lower them below the limits of `data/limits.json`, never raise them |
| `DEFAULT_MAX_RESOLVER_CALLS`, `DEFAULT_MAX_WARNINGS` | gone: 0.2 has no resolver-call or warning budget |
| `LintWarning`, `accessibility`, `AccessibilityDiagnostic`, `AccessibilitySeverity` | `api::lint` returns `api::Entry` rows; the accessibility checks are catalog rules |
| `Options::lint_caption_looking_paragraph` | the catalog rule `caption-looking-paragraph`. It is on by default, where the 0.1 option was off; turn it off with `LintOptions::rules` (`"caption-looking-paragraph"` to `"off"`) |
| `html_import`, `html_to_carve`, `HtmlImportOptions`, `HtmlImportMode`, `HtmlImportResult`, `HtmlImportReport`, `HtmlImportError`, `HtmlImportDiagnostic`, `HtmlImportDiagnosticCode`, `HtmlImportSeverity`, `MAX_HTML_IMPORT_DEPTH` | `api::import_document` with `ImportFormat::Html` and `ImportOptions` (`raw_html`, `strict_losses`, `adapter`, `list_table`). It returns `Imported { tree, source, diagnostics, suppressed_diagnostics }` |
| `HtmlImportAdapter` | `api::ImportAdapter` |
| `markdown_import`, `markdown_to_ast`, `markdown_to_carve`, `try_markdown_to_carve`, `migrate_markdown`, `djot_migrate`, `SourceFormat` | `api::import_document` or `Carve::import_from` with `ImportFormat::Markdown`, `Gfm` or `Djot` |
| `migrate_case_only_references`, `migration_raw_heading_ids`, `fragment_fixup_diagnostic` | gone: the import decides these itself and reports them in `Imported::diagnostics` |
| `MigrationReport`, `MigrationDiagnostic`, `MigrationConfidence`, `MigrationFidelity`, `ImportConfidence`, `ImportFidelity`, `conversion_diagnostics`, `ConversionDiagnostic`, `ConversionDiagnosticCode`, `ConversionDiagnostics`, `ConversionPos`, `to_conversion_diagnostics_json`, `DEFAULT_MAX_CONVERSION_DIAGNOSTICS` | `Imported::diagnostics` and `suppressed_diagnostics`; the directory run writes `migration-report.json` |
| `bbcode_migrate`, `BbcodeImportError`, `BBCODE_MAX_INPUT_LENGTH` | gone: BBCode import stays in carve-js |
| `CarveExtension`, `BeforeRenderContext`, `BlockMatch`, `InlineMatch`, `MatcherContext`, `StaticRenderers` | `api::Extension`, `ExtensionDeclaration`, `Renderer`, `RenderContext` and `DocumentTransform`, registered with `Carve::register_extension` (provisional) |
| `Autolink`, `Citations`, `CodeCallouts`, `CodeGroup`, `ColorSwatch`, `ExternalLinks`, `FencedRender`, `Glossary`, `HeadingNumbers`, `HeadingPermalinks`, `HeadingReference`, `Index`, `ListTable`, `MathBlock`, `SmartQuotes`, `Spoiler`, `TableOfContents`, `TocPlacement`, `TabNormalize`, `Wikilinks` | the factories of `api::extensions`: `autolink`, `citations`, `code_callouts`, `code_group`, `color_swatch`, `external_links`, `fenced_render`, `glossary`, `heading_numbers`, `heading_permalinks`, `heading_reference`, `index`, `list_table`, `math_block`, `smart_quotes`, `spoiler`, `table_of_contents`, `toc_placement`, `tab_normalize`, `wikilinks` |
| `Tabs`, `TabsMode`, `TabsOptions`, `ImgFence` | the standard extensions `carve.tabs` and `carve.imgFence.img`, enabled with `Carve::enable_standard`; `Carve::enable_standard_tabs_renderer` takes the tabs mode |
| `HeadingLevelShift`, `HeadingLevelShiftOptions` | `Options::heading_shift` |
| `CslEntry`, `CslName`, `CslDate` | `CitationsOptions::bibliography`, a list of CSL-JSON values |
| `SocialLinkKind`, `SocialLinkResolver`, `SocialLinkResolverInput` | `api::Social` and `Options::social_resolvers` (`SocialResolvers`, `SocialRequest`) |
| `parse_experimental_event_blocks`, `parse_experimental_event_blocks_with_options` | gone, together with the experimental event parser and its two Cargo flags |
| `Stamp`, `StampForm` | gone: version stamps belong to the 0.1 upgrade flow |

## Command line

The flags below were in the 0.1 `carve` command and have no row in [`spec/0.2/UPGRADING.md`](https://github.com/markup-carve/carve/blob/main/spec/0.2/UPGRADING.md), which maps the other removed flags (`--ast`, `--json`, `--safe`, `--static`, `--citation-mode`, `--tabs-mode` and more). `carve <command> --help` lists the 0.2 flags.

| 0.1 flag | 0.2 |
|---|---|
| `--html`, `--markdown`, `--md`, `--plain`, `--plain-text`, `--ansi` | `render --to html\|markdown\|plain\|ansi` |
| `--carve`, `--to carve` | `carve fmt` writes Carve source; `carve flatten` writes it with includes expanded |
| `--interactive` | `render --mode interactive` (the default); `--mode static` replaces `--static` |
| `--max-depth`, `--max-bytes` (render, fmt, flatten, lint) | gone: the command uses the include limits of `data/limits.json`, 16 levels and 8 MiB. A library host lowers them with `Includes::depth` and `Includes::bytes` |
| `--max-resolver-calls`, `--max-warnings` (render, fmt, flatten, lint) | gone: no resolver-call or warning budget in 0.2 |
| `--profile-base-host` | gone from the command line; library hosts set `LinkPolicyOptions::base_host` on `api::extensions::link_policy` |
| `--stamp`, `--stamp-block` (fmt), `--stamp-info`, `--stamp-check` | gone: version stamps belong to the 0.1 upgrade flow |
| `carve migrate` | `carve import --from html\|markdown\|gfm\|djot`; a Markdown folder is `carve import --from gfm --directory SOURCE --output TARGET` |
