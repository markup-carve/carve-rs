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
    /// names a format of [`super::ImportFormat`]; `rawHtml` and
    /// `strictLosses` are booleans.
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
                    Some(name) if super::ImportFormat::from_name(name).is_some() => {}
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

/// `allowLosses` and `maxLosses` as the importer reads them (C2-H-204,
/// C2-H-205). Apart from [`ImportOptions`], which stays `Copy`: a list of
/// codes read at run time has no `Copy` form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LossOptions {
    /// Loss codes left out of the record; they count for neither
    /// `strict_losses` nor `max_losses`.
    pub allow_losses: Vec<String>,
    /// The number of losses the record keeps, 100 by default.
    pub max_losses: usize,
}

impl Default for LossOptions {
    fn default() -> Self {
        Self {
            allow_losses: Vec::new(),
            max_losses: crate::options::Options::default().max_losses,
        }
    }
}

/// The imported tree, its canonical Carve source, and the record of the
/// importer followed by that of the writer (C2-M-001, C2-T-005).
#[derive(Debug, Clone)]
pub struct Imported {
    pub tree: Tree,
    pub source: String,
    pub diagnostics: Vec<Entry>,
    /// The losses past `max_losses` (C2-H-205).
    pub suppressed_diagnostics: usize,
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
        self.import_document_with(html, opts, &LossOptions::default())
    }

    /// [`Carve::import_document`] with `allowLosses` and `maxLosses`
    /// (C2-H-204, C2-H-205).
    ///
    /// # Errors
    ///
    /// As [`Carve::import_document`]; an allowed loss fails no strict import.
    pub fn import_document_with(
        &self,
        html: &str,
        opts: &ImportOptions,
        limits: &LossOptions,
    ) -> Result<Imported, Error> {
        let imported = run(html, opts.raw_html)?;
        self.finish_import(imported, *opts, limits)
    }

    /// `importDocument` of Markdown, GFM, Djot or HTML (carve-js#5074): the
    /// library HTML of the input through the HTML importer, raw HTML kept.
    ///
    /// # Errors
    ///
    /// As [`Carve::import_document_with`].
    pub fn import_from(
        &self,
        input: &str,
        format: super::ImportFormat,
        opts: &ImportOptions,
        limits: &LossOptions,
    ) -> Result<Imported, Error> {
        if format == super::ImportFormat::Html {
            return self.import_document_with(input, opts, limits);
        }
        let mut sink = super::hub::Sink::default();
        let html = super::hub::to_html(input, format, &mut sink)?;
        let mut imported = run(&html, true)?;
        imported.tree = super::hub::finish(imported.tree, &sink);
        // The library's own line breaks are no whitespace of the input, and a
        // range would index the intermediate HTML.
        imported
            .diagnostics
            .retain(|e| e.code != "import-whitespace-normalized");
        for e in &mut imported.diagnostics {
            if e.code.starts_with("import-") {
                e.range = None;
            }
        }
        self.finish_import(imported, *opts, limits)
    }

    fn finish_import(
        &self,
        imported: crate::import::Imported,
        opts: ImportOptions,
        limits: &LossOptions,
    ) -> Result<Imported, Error> {
        // The importer builds this tree from checked nodes and applies its own
        // depth limits. Avoid serializing and re-reading it as external JSON.
        let tree = Tree::trusted(imported.tree, None).map_err(|e| Error::ImportInput {
            message: format!("the imported tree cannot be decoded: {e}"),
        })?;
        let allowed = |e: &Entry| limits.allow_losses.contains(&e.code);
        let mut diagnostics = imported.diagnostics;
        diagnostics.retain(|e| !(e.channel == "render-loss" && allowed(e)));
        let written = match self.write_with_diagnostics(&tree, &super::WriteOptions::default()) {
            Err(Error::Write { .. }) if opts.strict_losses => {
                let mut codes = losses(&diagnostics);
                codes.push("writer-refused".into());
                return Err(Error::Render { codes });
            }
            other => other?,
        };
        diagnostics.extend(
            written
                .diagnostics
                .into_iter()
                .filter(|e| !(e.channel == "render-loss" && allowed(e))),
        );
        if opts.strict_losses {
            let codes = losses(&diagnostics);
            if !codes.is_empty() {
                return Err(Error::Render { codes });
            }
        }
        let mut kept = 0usize;
        let mut suppressed_diagnostics = 0usize;
        diagnostics.retain(|e| {
            if e.channel != "render-loss" {
                return true;
            }
            kept += 1;
            suppressed_diagnostics += usize::from(kept > limits.max_losses);
            kept <= limits.max_losses
        });
        Ok(Imported {
            tree,
            source: written.output,
            diagnostics,
            suppressed_diagnostics,
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
