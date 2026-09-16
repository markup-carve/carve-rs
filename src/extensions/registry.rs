//! Name-keyed registry of the built-in extensions.
//!
//! Bindings (carve-py, carve-rb, carve-wasm) expose extensions by name, and
//! before this registry each of them re-typed the list by hand. Nothing failed
//! when an extension landed here and a binding did not learn about it, so the
//! lists drifted quietly - carve-py sat ten extensions behind.
//!
//! The list therefore lives here, once, next to the extensions themselves:
//!
//! ```
//! let ext = carve::extensions::registry::by_key("autolink").unwrap();
//! assert_eq!(ext.name(), "autolink");
//! assert!(carve::extensions::registry::keys().any(|k| k == "glossary"));
//! ```
//!
//! A key is not always an extension's own [`CarveExtension::name`]. The names
//! are historical and inconsistent (`codeCallouts`, `headingNumbers`, `color`),
//! and the eight `fenced-render` presets all answer `fenced-render`, so they
//! cannot be told apart by name at all. Keys are kebab-case and unique, which
//! `registry_keys_are_kebab_case_and_unique` enforces.
//!
//! Which is also why an entry carries its own [`Registered::diagram_key`]: the
//! static-renderer key is the other thing `name()` cannot answer.

use crate::citations::Citations;
use crate::extensions::{
    Autolink, CodeCallouts, CodeGroup, ColorSwatch, Details, ExternalLinks, FencedRender, Glossary,
    HeadingLevelShift, HeadingNumbers, HeadingPermalinks, HeadingReference, ImgFence, Index,
    ListTable, MathBlock, SemanticSpan, SmartQuotes, Spoiler, TabNormalize, TableOfContents, Tabs,
    TocPlacement, Wikilinks,
};
use crate::CarveExtension;

/// One registry entry: the key callers name, the module it lives in, and a
/// constructor for a default-configured instance.
pub struct Registered {
    /// The stable, kebab-case name a caller passes.
    pub key: &'static str,
    /// The module the extension is defined in. Carried so a test can compare
    /// the registry against the modules on disk; see
    /// `every_extension_module_is_registered`.
    pub module: &'static str,
    /// The `StaticRenderers::diagram` key this extension is consulted under,
    /// `None` for one that is never consulted. A binding validates a caller's
    /// `renderers.diagrams` keys against these rather than against a copy.
    pub diagram_key: Option<&'static str>,
    factory: fn() -> Box<dyn CarveExtension>,
}

impl Registered {
    /// Build a default-configured instance of this extension.
    ///
    /// An extension that takes options is built with its own defaults. A caller
    /// that needs non-default options constructs the type directly - the
    /// registry is the name-keyed path, not a configuration API.
    pub fn build(&self) -> Box<dyn CarveExtension> {
        (self.factory)()
    }
}

/// Every built-in extension, in key order.
pub static REGISTRY: &[Registered] = &[
    Registered {
        key: "autolink",
        module: "autolink",
        diagram_key: None,
        factory: || Box::new(Autolink::new()),
    },
    Registered {
        key: "citations",
        module: "citations",
        diagram_key: None,
        factory: || Box::new(Citations::new()),
    },
    Registered {
        key: "code-callouts",
        module: "code_callouts",
        diagram_key: None,
        factory: || Box::new(CodeCallouts::new()),
    },
    Registered {
        key: "code-group",
        module: "code_group",
        diagram_key: None,
        factory: || Box::new(CodeGroup::new()),
    },
    Registered {
        key: "color-swatch",
        module: "color_swatch",
        diagram_key: None,
        factory: || Box::new(ColorSwatch::new()),
    },
    Registered {
        key: "details",
        module: "details",
        diagram_key: None,
        factory: || Box::new(Details::new()),
    },
    Registered {
        key: "external-links",
        module: "external_links",
        diagram_key: None,
        factory: || Box::new(ExternalLinks::new()),
    },
    // The mermaid preset carries the static-renderer key, so a static render
    // can consult a supplied renderer. A plain `FencedRender::new("mermaid")`
    // would degrade to source even with one, having no static-renderer key.
    Registered {
        key: "fenced-render",
        module: "fenced_render",
        diagram_key: Some("mermaid"),
        factory: || Box::new(FencedRender::mermaid()),
    },
    Registered {
        key: "fenced-render-abc",
        module: "fenced_render",
        diagram_key: Some("abc"),
        factory: || Box::new(FencedRender::abc()),
    },
    Registered {
        key: "fenced-render-chart",
        module: "fenced_render",
        diagram_key: Some("chart"),
        factory: || Box::new(FencedRender::chart()),
    },
    Registered {
        key: "fenced-render-d2",
        module: "fenced_render",
        diagram_key: Some("d2"),
        factory: || Box::new(FencedRender::d2()),
    },
    Registered {
        key: "fenced-render-graphviz",
        module: "fenced_render",
        diagram_key: Some("graphviz"),
        factory: || Box::new(FencedRender::graphviz()),
    },
    Registered {
        key: "fenced-render-plantuml",
        module: "fenced_render",
        diagram_key: Some("plantuml"),
        factory: || Box::new(FencedRender::plantuml()),
    },
    Registered {
        key: "fenced-render-vega-lite",
        module: "fenced_render",
        diagram_key: Some("vega-lite"),
        factory: || Box::new(FencedRender::vega_lite()),
    },
    Registered {
        key: "fenced-render-wavedrom",
        module: "fenced_render",
        diagram_key: Some("wavedrom"),
        factory: || Box::new(FencedRender::wavedrom()),
    },
    Registered {
        key: "glossary",
        module: "glossary",
        diagram_key: None,
        factory: || Box::new(Glossary::new()),
    },
    Registered {
        key: "heading-level-shift",
        module: "heading_level_shift",
        diagram_key: None,
        factory: || Box::new(HeadingLevelShift::new()),
    },
    Registered {
        key: "heading-numbers",
        module: "heading_numbers",
        diagram_key: None,
        factory: || Box::new(HeadingNumbers::new()),
    },
    Registered {
        key: "heading-permalinks",
        module: "heading_permalinks",
        diagram_key: None,
        factory: || Box::new(HeadingPermalinks::new()),
    },
    Registered {
        key: "heading-reference",
        module: "heading_reference",
        diagram_key: None,
        factory: || Box::new(HeadingReference::new()),
    },
    Registered {
        key: "img-fence",
        module: "img_fence",
        diagram_key: None,
        factory: || Box::new(ImgFence::new()),
    },
    Registered {
        key: "index",
        module: "index_terms",
        diagram_key: None,
        factory: || Box::new(Index::new()),
    },
    Registered {
        key: "list-table",
        module: "list_table",
        diagram_key: None,
        factory: || Box::new(ListTable::new()),
    },
    Registered {
        key: "math-block",
        module: "math_block",
        diagram_key: None,
        factory: || Box::new(MathBlock::new()),
    },
    // Locale-aware, and the registry has no way to carry a locale, so the key
    // builds the `en` quotes. Another locale means constructing `SmartQuotes`
    // directly with it; `SMART_QUOTE_LOCALES` lists what is supported.
    Registered {
        key: "smart-quotes",
        module: "smart_quotes",
        diagram_key: None,
        factory: || Box::new(SmartQuotes::new("en")),
    },
    Registered {
        key: "semantic-span",
        module: "semantic_span",
        diagram_key: None,
        factory: || Box::new(SemanticSpan),
    },
    Registered {
        key: "spoiler",
        module: "spoiler",
        diagram_key: None,
        factory: || Box::new(Spoiler::new()),
    },
    Registered {
        key: "tabs",
        module: "tabs",
        diagram_key: None,
        factory: || Box::new(Tabs::new()),
    },
    Registered {
        key: "tab-normalize",
        module: "tab_normalize",
        diagram_key: None,
        factory: || Box::new(TabNormalize::new()),
    },
    Registered {
        key: "table-of-contents",
        module: "table_of_contents",
        diagram_key: None,
        factory: || Box::new(TableOfContents::new()),
    },
    // `::: toc` placement, a second extension in the same module: it places a
    // table the `table-of-contents` extension collected.
    Registered {
        key: "toc",
        module: "table_of_contents",
        diagram_key: None,
        factory: || Box::new(TocPlacement::new()),
    },
    Registered {
        key: "wikilinks",
        module: "wikilinks",
        diagram_key: None,
        factory: || Box::new(Wikilinks::new()),
    },
];

/// Every registered key, in registry order.
pub fn keys() -> impl Iterator<Item = &'static str> {
    REGISTRY.iter().map(|entry| entry.key)
}

/// Every static-renderer diagram key, in registry order.
///
/// A binding validates a caller's `renderers.diagrams` keys against this rather
/// than against a list of its own.
pub fn diagram_keys() -> impl Iterator<Item = &'static str> {
    REGISTRY.iter().filter_map(|entry| entry.diagram_key)
}

/// Build the extension registered under `key`, or `None` when no such key
/// exists. Unknown keys are the caller's to report - a binding wants to say
/// which names it does accept.
pub fn by_key(key: &str) -> Option<Box<dyn CarveExtension>> {
    REGISTRY
        .iter()
        .find(|entry| entry.key == key)
        .map(Registered::build)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn every_key_builds() {
        for entry in REGISTRY {
            let ext = entry.build();
            assert!(
                !ext.name().is_empty(),
                "{} built an extension with no name",
                entry.key
            );
        }
    }

    #[test]
    fn registry_keys_are_kebab_case_and_unique() {
        let mut seen = HashSet::new();
        for entry in REGISTRY {
            assert!(
                seen.insert(entry.key),
                "duplicate registry key {:?}",
                entry.key
            );
            assert!(
                entry
                    .key
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "registry key {:?} is not kebab-case",
                entry.key
            );
        }
    }

    #[test]
    fn by_key_rejects_an_unknown_name() {
        assert!(by_key("no-such-extension").is_none());
    }

    /// The field is DERIVED-CHECKED, not hand-compared: a preset registered
    /// under the wrong class, or a new one registered with no class at all,
    /// fails here.
    #[test]
    fn every_published_diagram_key_is_the_built_extensions_own() {
        for entry in REGISTRY {
            let built = entry.build();
            assert_eq!(
                entry.diagram_key,
                built.diagram_key(),
                "registry key {:?} publishes a diagram key the extension does not have",
                entry.key
            );
        }
    }

    /// And the published set is every bundled preset, so a ninth preset added
    /// to `FencedRender::presets()` fails until it is registered.
    #[test]
    fn the_published_keys_are_every_bundled_preset() {
        let mut published: Vec<&str> = diagram_keys().collect();
        let presets = crate::FencedRender::presets();
        let mut from_presets: Vec<&str> = presets.iter().filter_map(|p| p.diagram_key()).collect();
        published.sort_unstable();
        from_presets.sort_unstable();
        assert_eq!(published, from_presets);
    }

    /// What makes the key worth publishing: it is the one `get_diagram` looks
    /// up. Driven per key, with a control under a name nothing is registered
    /// for, so a green row cannot come from static mode alone.
    #[test]
    fn a_published_key_is_the_one_the_static_renderer_is_found_under() {
        for entry in REGISTRY {
            let Some(key) = entry.diagram_key else {
                continue;
            };
            // The css class is also one of the words each preset claims, so it
            // doubles as the fence's info string.
            let source = format!("``` {key}\nBODY\n```\n");
            let ext = entry.build();

            let html = static_html(&source, ext.as_ref(), key);
            assert!(
                html.contains("<svg data-len=\"4\">"),
                "{key} never reached its static renderer: {html}"
            );

            let html = static_html(&source, ext.as_ref(), &format!("not-{key}"));
            assert!(
                !html.contains("<svg"),
                "CONTROL: {key} rendered through a renderer registered under another key: {html}"
            );
        }
    }

    fn static_html(source: &str, ext: &dyn CarveExtension, renderer_key: &str) -> String {
        let renderers = crate::StaticRenderers::new()
            .diagram(renderer_key.to_string(), |src: &str| {
                format!("<svg data-len=\"{}\"></svg>", src.len())
            });
        let opts = crate::Options::new()
            .with_mode(crate::Mode::Static)
            .with_renderers(renderers)
            .with_extension(ext);
        crate::to_html_with_options(source, &opts)
    }

    /// The check that makes this registry worth having.
    ///
    /// Reading the module list off disk is what lets the test fail: a hand-kept
    /// expected list would go stale in exactly the way the registry is meant to
    /// stop. A new file under `src/extensions/` fails this test until it is
    /// registered or named here as carrying no extension.
    #[test]
    fn every_extension_module_is_registered() {
        // Not extensions: `mod.rs` declares them, `registry.rs` is this file,
        // and `svg_sanitize` is a helper function used by other extensions.
        const NOT_AN_EXTENSION: &[&str] = &["mod", "registry", "svg_sanitize"];

        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/extensions");
        let mut on_disk = HashSet::new();
        for entry in std::fs::read_dir(&dir).expect("src/extensions is readable") {
            let path = entry.expect("a readable directory entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .expect("a UTF-8 file name")
                .to_string();
            if NOT_AN_EXTENSION.contains(&stem.as_str()) {
                continue;
            }
            on_disk.insert(stem);
        }

        let registered: HashSet<String> = REGISTRY.iter().map(|e| e.module.to_string()).collect();
        let missing: Vec<&String> = on_disk.difference(&registered).collect();
        assert!(
            missing.is_empty(),
            "extension modules with no registry entry: {missing:?} - add them to REGISTRY, \
             or to NOT_AN_EXTENSION if the module carries no extension"
        );
    }

    #[test]
    fn a_registered_extension_actually_runs() {
        let ext = by_key("autolink").expect("autolink is registered");
        let opts = crate::Options::new().with_extension(ext.as_ref());
        let html = crate::to_html_with_options("Visit https://example.com.", &opts);
        assert!(html.contains("<a href=\"https://example.com\""), "{html}");
    }
}
