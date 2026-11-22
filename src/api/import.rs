//! `importDocument` (C2-M-001, C2-T-005): HTML to a 0.2 tree and the record.

use super::{Entry, Error, Tree};
use serde_json::Value;

/// The editor that wrote the HTML, the option `adapter` (C2-M-102).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ImportAdapter {
    #[default]
    Generic,
    Tiptap,
    Prosemirror,
    Ckeditor,
    Tinymce,
    Word,
    GoogleDocs,
}

impl ImportAdapter {
    /// Every adapter, in the order of `data/import-html.json`'s option row.
    pub const ALL: [Self; 7] = [
        Self::Generic,
        Self::Tiptap,
        Self::Prosemirror,
        Self::Ckeditor,
        Self::Tinymce,
        Self::Word,
        Self::GoogleDocs,
    ];

    /// The option value of the adapter.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Generic => "generic",
            Self::Tiptap => "tiptap",
            Self::Prosemirror => "prosemirror",
            Self::Ckeditor => "ckeditor",
            Self::Tinymce => "tinymce",
            Self::Word => "word",
            Self::GoogleDocs => "google-docs",
        }
    }

    /// The adapter an option value names.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.name() == name)
    }

    /// C2-M-102: `word` and `google-docs` run the note pass; the others read
    /// the input by the mapping alone.
    #[must_use]
    pub const fn runs_note_pass(self) -> bool {
        matches!(self, Self::Word | Self::GoogleDocs)
    }
}

/// The options of [`import_document`] (C2-M-066, C2-M-082). The input format
/// is HTML; the JSON boundary names it with `from: "html"`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ImportOptions {
    /// Keeps comments and raw elements as raw nodes (C2-M-065, C2-M-066).
    pub raw_html: bool,
    /// Makes every loss of the import an error (C2-M-082, C2-H-130).
    pub strict_losses: bool,
    /// The editor adapter (C2-M-102).
    pub adapter: ImportAdapter,
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
                "adapter" => {
                    out.adapter = v
                        .as_str()
                        .and_then(ImportAdapter::from_name)
                        .ok_or_else(|| Error::option(name, format!("unknown value {v}")))?;
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

/// Import one GFM document for a directory migration. Link destinations are
/// checked after the entire output tree has been built.
///
/// # Errors
///
/// Returns an import or writer error if the document cannot be converted.
pub fn try_migrate_markdown_for_directory(source: &str) -> Result<Imported, Error> {
    super::Carve::new().import_from(
        source,
        super::ImportFormat::Gfm,
        &ImportOptions::default(),
        &LossOptions::default(),
    )
}

/// The directory import and its authored link destinations. The destination
/// map retains URL spellings that a later link audit may need to report.
///
/// # Errors
///
/// Returns an import or writer error if the document cannot be converted.
pub fn try_migrate_markdown_for_directory_with_destinations(
    source: &str,
) -> Result<(Imported, std::collections::HashMap<String, String>), Error> {
    let imported = try_migrate_markdown_for_directory(source)?;
    let mut destinations = std::collections::HashMap::new();
    let value: Value = serde_json::from_str(&super::encode_tree(&imported.tree)).map_err(|e| {
        Error::ImportInput {
            message: e.to_string(),
        }
    })?;
    collect_destinations(&value, &mut destinations);
    for (decoded, authored) in markdown_destinations(source) {
        if let Some(value) = destinations.get_mut(&decoded) {
            *value = authored;
        }
    }
    Ok((imported, destinations))
}

#[allow(clippy::cognitive_complexity)]
fn markdown_destinations(source: &str) -> Vec<(String, String)> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes.get(i) != Some(&b']') || bytes.get(i + 1) != Some(&b'(') {
            i += 1;
            continue;
        }
        let mut start = i + 2;
        while bytes.get(start).is_some_and(u8::is_ascii_whitespace) {
            start += 1;
        }
        let angle = bytes.get(start) == Some(&b'<');
        if angle {
            start += 1;
        }
        let mut end = start;
        let mut depth = 0usize;
        while let Some(&byte) = bytes.get(end) {
            if byte == b'\\' && bytes.get(end + 1).is_some() {
                end += 2;
                continue;
            }
            if angle {
                if byte == b'>' {
                    break;
                }
            } else {
                if byte == b'(' {
                    depth += 1;
                }
                if byte == b')' {
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                }
                if byte.is_ascii_whitespace() {
                    break;
                }
            }
            end += 1;
        }
        if let Some(authored) = source.get(start..end) {
            let mut decoded = String::new();
            let mut chars = authored.chars();
            while let Some(ch) = chars.next() {
                if ch == '\\' {
                    if let Some(next) = chars.next() {
                        decoded.push(next);
                    }
                } else {
                    decoded.push(ch);
                }
            }
            out.push((decoded, authored.to_owned()));
        }
        i = end.saturating_add(1);
    }
    out
}

fn collect_destinations(value: &Value, out: &mut std::collections::HashMap<String, String>) {
    match value {
        Value::Object(object) => {
            for key in ["href", "src"] {
                if let Some(destination) = object.get(key).and_then(Value::as_str) {
                    out.entry(destination.to_owned())
                        .or_insert_with(|| destination.to_owned());
                }
            }
            for child in object.values() {
                collect_destinations(child, out);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_destinations(item, out);
            }
        }
        _ => {}
    }
}

fn run(
    html: &str,
    raw_html: bool,
    adapter: ImportAdapter,
) -> Result<crate::import::Imported, Error> {
    let import = || {
        crate::import::import_html(html, raw_html, adapter.runs_note_pass())
            .map_err(|r| Error::ImportInput { message: r.0 })
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
        let imported = run(html, opts.raw_html, opts.adapter)?;
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
        let mut imported = run(&html, true, opts.adapter)?;
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
        // The importer already owns the checked JSON value. The public tree
        // needs its wire form, but the writer can read this value directly.
        let settings = crate::write::Settings {
            extensions: self
                .standard
                .tabs
                .then(|| "carve.tabs".to_string())
                .into_iter()
                .collect(),
            ..crate::write::Settings::default()
        };
        let (source, writer_diagnostics) = crate::write::write_value(&imported.tree, &settings);
        let tree = Tree::trusted(imported.tree, None).map_err(|e| Error::ImportInput {
            message: format!("the imported tree cannot be decoded: {e}"),
        })?;
        let allowed = |e: &Entry| limits.allow_losses.contains(&e.code);
        let mut diagnostics = imported.diagnostics;
        diagnostics.retain(|e| !(e.channel == "render-loss" && allowed(e)));
        diagnostics.extend(
            writer_diagnostics
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
            source,
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
