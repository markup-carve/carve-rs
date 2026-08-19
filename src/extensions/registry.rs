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
//! and the nine `fenced-render` presets all answer `fenced-render`, so they
//! cannot be told apart by name at all. Keys are kebab-case and unique, which
//! `registry_keys_are_kebab_case_and_unique` enforces.

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
        factory: || Box::new(Autolink::new()),
    },
    Registered {
        key: "citations",
        module: "citations",
        factory: || Box::new(Citations::new()),
    },
    Registered {
        key: "code-callouts",
        module: "code_callouts",
        factory: || Box::new(CodeCallouts::new()),
    },
    Registered {
        key: "code-group",
        module: "code_group",
        factory: || Box::new(CodeGroup::new()),
    },
    Registered {
        key: "color-swatch",
        module: "color_swatch",
        factory: || Box::new(ColorSwatch::new()),
    },
    Registered {
        key: "details",
        module: "details",
        factory: || Box::new(Details::new()),
    },
    Registered {
        key: "external-links",
        module: "external_links",
        factory: || Box::new(ExternalLinks::new()),
    },
    // The mermaid preset carries the static-renderer key, so a static render
    // can consult a supplied renderer. A plain `FencedRender::new("mermaid")`
    // would degrade to source even with one, having no static-renderer key.
    Registered {
        key: "fenced-render",
        module: "fenced_render",
        factory: || Box::new(FencedRender::mermaid()),
    },
    Registered {
        key: "fenced-render-abc",
        module: "fenced_render",
        factory: || Box::new(FencedRender::abc()),
    },
    Registered {
        key: "fenced-render-chart",
        module: "fenced_render",
        factory: || Box::new(FencedRender::chart()),
    },
    Registered {
        key: "fenced-render-d2",
        module: "fenced_render",
        factory: || Box::new(FencedRender::d2()),
    },
    Registered {
        key: "fenced-render-graphviz",
        module: "fenced_render",
        factory: || Box::new(FencedRender::graphviz()),
    },
    Registered {
        key: "fenced-render-plantuml",
        module: "fenced_render",
        factory: || Box::new(FencedRender::plantuml()),
    },
    Registered {
        key: "fenced-render-vega-lite",
        module: "fenced_render",
        factory: || Box::new(FencedRender::vega_lite()),
    },
    Registered {
        key: "fenced-render-wavedrom",
        module: "fenced_render",
        factory: || Box::new(FencedRender::wavedrom()),
    },
    Registered {
        key: "glossary",
        module: "glossary",
        factory: || Box::new(Glossary::new()),
    },
    Registered {
        key: "heading-level-shift",
        module: "heading_level_shift",
        factory: || Box::new(HeadingLevelShift::new()),
    },
    Registered {
        key: "heading-numbers",
        module: "heading_numbers",
        factory: || Box::new(HeadingNumbers::new()),
    },
    Registered {
        key: "heading-permalinks",
        module: "heading_permalinks",
        factory: || Box::new(HeadingPermalinks::new()),
    },
    Registered {
        key: "heading-reference",
        module: "heading_reference",
        factory: || Box::new(HeadingReference::new()),
    },
    Registered {
        key: "img-fence",
        module: "img_fence",
        factory: || Box::new(ImgFence::new()),
    },
    Registered {
        key: "index",
        module: "index_terms",
        factory: || Box::new(Index::new()),
    },
    Registered {
        key: "list-table",
        module: "list_table",
        factory: || Box::new(ListTable::new()),
    },
    Registered {
        key: "math-block",
        module: "math_block",
        factory: || Box::new(MathBlock::new()),
    },
    // Locale-aware, and the registry has no way to carry a locale, so the key
    // builds the `en` quotes. Another locale means constructing `SmartQuotes`
    // directly with it; `SMART_QUOTE_LOCALES` lists what is supported.
    Registered {
        key: "smart-quotes",
        module: "smart_quotes",
        factory: || Box::new(SmartQuotes::new("en")),
    },
    Registered {
        key: "semantic-span",
        module: "semantic_span",
        factory: || Box::new(SemanticSpan),
    },
    Registered {
        key: "spoiler",
        module: "spoiler",
        factory: || Box::new(Spoiler::new()),
    },
    Registered {
        key: "tabs",
        module: "tabs",
        factory: || Box::new(Tabs::new()),
    },
    Registered {
        key: "tab-normalize",
        module: "tab_normalize",
        factory: || Box::new(TabNormalize::new()),
    },
    Registered {
        key: "table-of-contents",
        module: "table_of_contents",
        factory: || Box::new(TableOfContents::new()),
    },
    // `::: toc` placement, a second extension in the same module: it places a
    // table the `table-of-contents` extension collected.
    Registered {
        key: "toc",
        module: "table_of_contents",
        factory: || Box::new(TocPlacement::new()),
    },
    Registered {
        key: "wikilinks",
        module: "wikilinks",
        factory: || Box::new(Wikilinks::new()),
    },
];

/// Every registered key, in registry order.
pub fn keys() -> impl Iterator<Item = &'static str> {
    REGISTRY.iter().map(|entry| entry.key)
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
