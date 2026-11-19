//! `importDocument` (C2-M-001, C2-T-005): HTML to a 0.2 tree and the record.

use super::{Entry, Error, Tree};
use serde_json::Value;

/// The options of [`import_document`] (C2-M-066, C2-M-082). The input format
/// is HTML; the JSON boundary names it with `from: "html"`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ImportOptions {
    /// Keeps comments and raw elements as raw nodes (C2-M-065, C2-M-066).
    pub raw_html: bool,
    /// Makes every loss of the import an error (C2-M-082, C2-H-130).
    pub strict_losses: bool,
}

impl ImportOptions {
    /// Reads the options object of the JSON boundary: `from` is required and
    /// must be `html`; `rawHtml` and `strictLosses` are booleans.
    ///
    /// # Errors
    ///
    /// [`Error::Option`] for a missing or unknown `from`, an unknown option or
    /// a value of the wrong type.
    pub fn from_json(value: Option<&Value>) -> Result<Self, Error> {
        let Some(value) = value else {
            return Err(Error::option("from", "required"));
        };
        let Some(map) = value.as_object() else {
            return Err(Error::option("options", "the options are not an object"));
        };
        let mut out = Self::default();
        for (name, v) in map {
            match name.as_str() {
                "from" => match v.as_str() {
                    Some("html") => {}
                    Some(other) => {
                        return Err(Error::option("from", format!("unknown value \"{other}\"")));
                    }
                    None => return Err(Error::option("from", "not a string")),
                },
                "rawHtml" | "strictLosses" => {
                    let Some(b) = v.as_bool() else {
                        return Err(Error::option(name, "not a boolean"));
                    };
                    if name == "rawHtml" {
                        out.raw_html = b;
                    } else {
                        out.strict_losses = b;
                    }
                }
                _ => return Err(Error::option(name, "unknown option")),
            }
        }
        if !map.contains_key("from") {
            return Err(Error::option("from", "required"));
        }
        Ok(out)
    }
}

/// The imported tree, its canonical Carve source, and the record of the
/// importer followed by that of the writer (C2-M-001, C2-T-005).
#[derive(Debug, Clone)]
pub struct Imported {
    pub tree: Tree,
    pub source: String,
    pub diagnostics: Vec<Entry>,
}

fn run(html: &str, raw_html: bool) -> Result<crate::import::Imported, Error> {
    let import = || {
        crate::import::import_html(html, raw_html).map_err(|r| Error::ImportInput { message: r.0 })
    };
    // wasm32 targets have no threads; the default stack of the host applies.
    if cfg!(target_family = "wasm") {
        return import();
    }
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(256 * 1024 * 1024)
            .spawn_scoped(scope, import)
            .map_err(|e| Error::ImportInput {
                message: e.to_string(),
            })?
            .join()
            .map_err(|_| Error::ImportInput {
                message: "the import failed".into(),
            })?
    })
}

/// `importDocument` (C2-M-001) on `Carve::default()`.
///
/// # Errors
///
/// As [`super::Carve::import_document`].
pub fn import_document(html: &str, opts: &ImportOptions) -> Result<Imported, Error> {
    super::Carve::default().import_document(html, opts)
}

impl super::Carve {
    /// `importDocument` (C2-M-001): the tree of an HTML input, its source
    /// written by the `carve` writer of this host, and the record.
    ///
    /// # Errors
    ///
    /// [`Error::ImportInput`] for an input the importer refuses (C2-M-006);
    /// [`Error::Render`] for an importer or writer loss under `strict_losses`
    /// (C2-M-082, C2-T-034); [`Error::Write`] for a writer loss without it.
    pub fn import_document(&self, html: &str, opts: &ImportOptions) -> Result<Imported, Error> {
        let imported = run(html, opts.raw_html)?;
        let tree = Tree::from_value(&imported.tree).map_err(|e| Error::ImportInput {
            message: format!("the imported tree fails the reader checks: {e}"),
        })?;
        let mut diagnostics = imported.diagnostics;
        let written = match self.write_with_diagnostics(&tree, &super::WriteOptions::default()) {
            Err(Error::Write { .. }) if opts.strict_losses => {
                let mut codes = losses(&diagnostics);
                codes.push("writer-refused".into());
                return Err(Error::Render { codes });
            }
            other => other?,
        };
        diagnostics.extend(written.diagnostics);
        if opts.strict_losses {
            let codes = losses(&diagnostics);
            if !codes.is_empty() {
                return Err(Error::Render { codes });
            }
        }
        Ok(Imported {
            tree,
            source: written.output,
            diagnostics,
        })
    }
}

fn losses(entries: &[Entry]) -> Vec<String> {
    entries
        .iter()
        .filter(|e| e.channel == "render-loss")
        .map(|e| e.code.clone())
        .collect()
}
