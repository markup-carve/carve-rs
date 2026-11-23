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

fn markdown_destinations(source: &str) -> Vec<(String, String)> {
    use comrak::nodes::NodeValue;
    let arena = comrak::Arena::new();
    let options = super::hub::markdown_options(source, true);
    let document = comrak::parse_document(&arena, source, &options);
    let lines: Vec<&str> = source.split('\n').collect();
    let mut starts = vec![0];
    for (at, byte) in source.bytes().enumerate() {
        if byte == b'\n' {
            starts.push(at + 1);
        }
    }
    let mut result = Vec::new();
    let mut code_lines = vec![false; lines.len() + 1];
    let mut referenced = std::collections::HashSet::new();
    for node in document.descendants() {
        let data = node.data.borrow();
        if matches!(data.value, NodeValue::CodeBlock(_)) {
            for line in data.sourcepos.start.line..=data.sourcepos.end.line.min(lines.len()) {
                if let Some(slot) = code_lines.get_mut(line) {
                    *slot = true;
                }
            }
        }
        let destination = match &data.value {
            NodeValue::Link(link) | NodeValue::Image(link) => &link.url,
            _ => continue,
        };
        referenced.insert(destination.to_owned());
        if let Some(raw) = node_source(source, &starts, data.sourcepos) {
            if let Some(authored) = inline_destination(raw, destination) {
                result.push((destination.to_owned(), authored));
            }
        }
    }
    for (line_number, line) in lines.iter().enumerate() {
        if code_lines.get(line_number + 1) == Some(&true) {
            continue;
        }
        if let Some((decoded, authored)) = reference_destination(line) {
            if referenced.contains(&decoded) {
                result.push((decoded, authored));
            }
        }
    }
    result
}

fn node_source<'a>(
    source: &'a str,
    starts: &[usize],
    pos: comrak::nodes::Sourcepos,
) -> Option<&'a str> {
    let start = starts
        .get(pos.start.line.checked_sub(1)?)?
        .checked_add(pos.start.column.checked_sub(1)?)?;
    let end = starts
        .get(pos.end.line.checked_sub(1)?)?
        .checked_add(pos.end.column)?;
    let bounded = source.get(start..end)?;
    if bounded.contains("](") && !bounded.ends_with(')') {
        source.get(start..)
    } else {
        Some(bounded)
    }
}

fn inline_destination(raw: &str, destination: &str) -> Option<String> {
    let bytes = raw.as_bytes();
    for i in 0..bytes.len().saturating_sub(1) {
        if bytes.get(i) == Some(&b']') && bytes.get(i + 1) == Some(&b'(') {
            let (_, authored, angle) = markdown_destination(raw, i + 2)?;
            if normalized_destination(&authored, angle).as_deref() == Some(destination) {
                return Some(authored);
            }
        }
    }
    None
}

fn reference_destination(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim_start_matches(' ');
    if line.len() - trimmed.len() > 3 || !trimmed.starts_with('[') {
        return None;
    }
    let (_, after) = trimmed
        .split_once("]: ")
        .or_else(|| trimmed.split_once("]:"))?;
    let raw = after.trim_start();
    let (authored, angle) = if let Some(value) = raw.strip_prefix('<') {
        (value.split_once('>')?.0, true)
    } else {
        (raw.split(char::is_whitespace).next()?, false)
    };
    Some((
        normalized_destination(authored, angle)?,
        authored.to_owned(),
    ))
}

fn normalized_destination(authored: &str, angle: bool) -> Option<String> {
    let probe = if angle {
        format!("[x](<{authored}>)")
    } else {
        format!("[x]({authored})")
    };
    let arena = comrak::Arena::new();
    let document = comrak::parse_document(&arena, &probe, &comrak::Options::default());
    document.descendants().find_map(|node| {
        if let comrak::nodes::NodeValue::Link(link) = &node.data.borrow().value {
            Some(link.url.clone())
        } else {
            None
        }
    })
}

fn markdown_destination(source: &str, after: usize) -> Option<(usize, String, bool)> {
    let bytes = source.as_bytes();
    let mut start = after;
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
        if angle && byte == b'>' {
            break;
        }
        if !angle {
            if byte == b'(' {
                depth += 1;
            } else if byte == b')' {
                if depth == 0 {
                    break;
                }
                depth -= 1;
            } else if byte.is_ascii_whitespace() {
                break;
            }
        }
        end += 1;
    }
    let authored = source.get(start..end)?;
    Some((end, authored.to_owned(), angle))
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
        // The importer already owns the checked JSON value. The writer reads it
        // directly; the public tree builds its wire form only when requested.
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
        let tree = Tree::trusted_imported(imported.tree).map_err(|e| Error::ImportInput {
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

#[cfg(test)]
mod imported_wire_test {
    use super::{ImportOptions, import_document};

    #[test]
    fn imported_tree_encodes_on_first_request() -> Result<(), String> {
        let html = "<ul><li><p>a <em>b</em></p></li></ul>";
        let expected = crate::import::import_html(html, false, true)
            .map_err(|error| error.0)?
            .tree
            .to_string();
        let imported =
            import_document(html, &ImportOptions::default()).map_err(|error| error.to_string())?;
        assert!(imported.tree.json.get().is_none());
        assert_eq!(crate::api::encode_tree(&imported.tree), expected);
        assert!(imported.tree.json.get().is_some());
        Ok(())
    }

    #[test]
    fn imported_tree_decodes_on_first_request() -> Result<(), String> {
        let html = "<h1>t</h1><table><tr><td><strong>a</strong></td></tr></table>";
        let value = crate::import::import_html(html, false, true)
            .map_err(|error| error.0)?
            .tree;
        let eager = crate::api::Tree::trusted(value, None).map_err(|error| error.to_string())?;
        let imported =
            import_document(html, &ImportOptions::default()).map_err(|error| error.to_string())?;
        assert!(imported.tree.document.document.get().is_none());
        assert_eq!(imported.tree.document(), eager.document());
        let opts = crate::api::Options {
            heading_shift: 1,
            ..crate::api::Options::default()
        };
        assert_eq!(
            crate::api::render_tree(&imported.tree, &opts).map_err(|error| error.to_string())?,
            crate::api::render_tree(&eager, &opts).map_err(|error| error.to_string())?
        );
        Ok(())
    }
}
