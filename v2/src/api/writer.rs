//! The writer registry: one interface for every output format that writes a
//! resolved 0.2 tree (C2-W-001). HTML is built in; the Carve writer, and the
//! Markdown, plain-text and ANSI writers ruled into 0.2, register here under
//! their target names when they are ported, without a change to this API.

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
        let rendered = crate::render_tree_with_diagnostics(tree.document(), &opts.render)?;
        Ok(Written {
            output: rendered.html,
            diagnostics: rendered.diagnostics.iter().map(Entry::from).collect(),
        })
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
    /// [`Error::Option`] when a writer already has the target name.
    pub fn register(&mut self, writer: Arc<dyn Writer>) -> Result<(), Error> {
        let target = writer.target().to_string();
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
