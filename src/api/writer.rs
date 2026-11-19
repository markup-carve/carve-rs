//! The writer registry: one interface for every output format that writes a
//! resolved 0.2 tree (C2-W-001). HTML and Carve are built in, and so are the
//! exports of [`crate::export`] under the reserved targets `markdown`,
//! `plain` and `ansi`.

use super::{Entry, Error, Options, Tree};
use std::collections::BTreeMap;
use std::sync::Arc;

/// The options every writer takes. `align_tables` and `normalize` are the
/// format options of `data/options.json`; `render` holds the render options a
/// writer such as HTML reads.
#[derive(Debug, Clone, Default)]
pub struct WriteOptions {
    pub align_tables: bool,
    pub normalize: bool,
    pub render: Options,
    /// The standard extensions the Carve writer enables, by name (`carve.tabs`).
    pub extensions: Vec<String>,
    /// The sidecar returned with the tree (C2-L-004), as its JSON value. The
    /// Carve writer reads its spellings (C2-W-083); `normalize` ignores it.
    pub layout: Option<serde_json::Value>,
}

/// The output of a writer and its record (C2-W-134).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Written {
    pub output: String,
    pub diagnostics: Vec<Entry>,
}

/// A writer of one target format. It refuses with [`Error::Write`] where
/// C2-W-134 makes a loss an error.
pub trait Writer: Send + Sync {
    /// The target name, such as `html`, `carve` or `markdown`.
    fn target(&self) -> &str;

    /// Writes the tree.
    ///
    /// # Errors
    ///
    /// [`Error::Write`] for a refused tree; any error of the calls it uses.
    fn write(&self, tree: &Tree, opts: &WriteOptions) -> Result<Written, Error>;
}

/// The built-in HTML writer: the render of a supplied tree (C2-A-135).
#[derive(Debug, Clone, Copy, Default)]
pub struct HtmlWriter;

impl Writer for HtmlWriter {
    #[allow(clippy::unnecessary_literal_bound)] // the trait lets other writers borrow
    fn target(&self) -> &str {
        "html"
    }

    fn write(&self, tree: &Tree, opts: &WriteOptions) -> Result<Written, Error> {
        let rendered = super::render_tree_with_diagnostics(tree, &opts.render)?;
        Ok(Written {
            output: rendered.html,
            diagnostics: rendered.diagnostics,
        })
    }
}

/// The built-in Carve writer (`writer.md`): canonical source for a tree, and
/// the losses of C2-W-011 in its record. [`super::Carve::write`] refuses with
/// [`Error::Write`] where that record holds one (C2-W-134).
#[derive(Debug, Clone, Copy, Default)]
pub struct CarveWriter;

impl Writer for CarveWriter {
    #[allow(clippy::unnecessary_literal_bound)] // the trait lets other writers borrow
    fn target(&self) -> &str {
        "carve"
    }

    fn write(&self, tree: &Tree, opts: &WriteOptions) -> Result<Written, Error> {
        check_extensions(&opts.extensions)?;
        let doc = tree_value(&tree.json)?;
        let settings = crate::write::Settings {
            align_tables: opts.align_tables,
            extensions: opts.extensions.clone(),
            layout: None,
        }
        .with_layout(opts.layout.as_ref().filter(|_| !opts.normalize));
        let (output, diagnostics) = crate::write::write_value(&doc, &settings);
        Ok(Written {
            output,
            diagnostics,
        })
    }
}

/// The JSON value of a tree's wire form; trees nest past the default recursion limit.
pub(crate) fn tree_value(json: &str) -> Result<serde_json::Value, Error> {
    let mut de = serde_json::Deserializer::from_str(json);
    de.disable_recursion_limit();
    serde::Deserialize::deserialize(&mut de).map_err(|e: serde_json::Error| Error::Write {
        target: "carve".into(),
        message: e.to_string(),
    })
}

/// The standard extensions this engine knows (`data/extensions.json`).
pub(crate) fn check_extensions(names: &[String]) -> Result<(), Error> {
    match names
        .iter()
        .find(|n| !matches!(n.as_str(), "carve.tabs" | "carve.tab"))
    {
        Some(n) => Err(Error::option(
            "extensions",
            format!("unknown standard extension {n}"),
        )),
        None => Ok(()),
    }
}

/// Writers by target name. [`Writers::default`] holds the built-in writers.
#[derive(Clone)]
pub struct Writers {
    by_target: BTreeMap<String, Arc<dyn Writer>>,
}

impl Default for Writers {
    fn default() -> Self {
        let mut by_target: BTreeMap<String, Arc<dyn Writer>> = BTreeMap::new();
        by_target.insert("html".into(), Arc::new(HtmlWriter));
        by_target.insert("carve".into(), Arc::new(CarveWriter));
        for target in crate::export::EXPORT_TARGETS {
            by_target.insert(
                target.name().into(),
                Arc::new(crate::export::ExportWriter(target)),
            );
        }
        Self { by_target }
    }
}

impl std::fmt::Debug for Writers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.by_target.keys()).finish()
    }
}

impl Writers {
    /// Adds a writer.
    ///
    /// # Errors
    ///
    /// [`Error::Option`] when a writer already has the target name, or the
    /// name is one the exports reserve (`markdown`, `plain`, `ansi`).
    pub fn register(&mut self, writer: Arc<dyn Writer>) -> Result<(), Error> {
        let target = writer.target().to_string();
        if crate::export::Target::from_name(&target).is_some() {
            return Err(Error::option(
                "writers",
                format!("the target {target} is reserved"),
            ));
        }
        if self.by_target.contains_key(&target) {
            return Err(Error::option(
                "writers",
                format!("a writer for {target} is already registered"),
            ));
        }
        self.by_target.insert(target, writer);
        Ok(())
    }

    /// The registered target names, sorted.
    #[must_use]
    pub fn targets(&self) -> Vec<&str> {
        self.by_target.keys().map(String::as_str).collect()
    }

    /// The writer of a target.
    #[must_use]
    pub fn get(&self, target: &str) -> Option<&Arc<dyn Writer>> {
        self.by_target.get(target)
    }
}
