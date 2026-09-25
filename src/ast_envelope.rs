//! The versioned AST interchange envelope (PART 12 §34, [CARVE-P12-056]).
//!
//! The tree is strict-closed and carries no version, so a reader that cannot
//! read a payload has one answer for three different problems: a corrupt tree,
//! a vocabulary this build does not know, and a document needing an extension
//! it does not implement all arrive as one decode error. The envelope is what
//! lets the reader name which one it hit.
//!
//! The tree does not move. `document` is exactly what [`crate::to_json`] writes
//! and [`crate::from_json`] reads; `carve --json` still writes it bare. This is
//! the storage-and-process-boundary surface beside that codec, not a change to
//! it.

use std::cmp::Ordering;
use std::fmt;

use crate::ast::Document;
use crate::ast_json::{from_json, parse_value, try_to_json, value_to_json, AstJsonError, Json};

/// The interchange contract version this build implements.
///
/// NOT the Carve language version, and it does not track it: the language is
/// versioned for authors, this is versioned for readers of a tree. A major bump
/// removes, renames or reinterprets a field or a meaning; a minor bump adds a
/// field or a node type.
pub const AST_CONTRACT_VERSION: &str = "1.0";

/// The vocabulary an envelope with no `vocabulary` is written in.
pub const CORE_AST_VOCABULARY: &str = "https://markup-carve.org/ast/core";

const ENVELOPE_FIELDS: [&str; 4] = ["astVersion", "vocabulary", "extensions", "document"];
const EXTENSION_FIELDS: [&str; 3] = ["id", "version", "required"];

/// `^[1-9][0-9]*\.(0|[1-9][0-9]*)$` from `ast-envelope-schema.json`. A leading
/// zero is refused, so the `0.x`-reads-as-major convention the rest of this org
/// versions by never applies to the contract version.
fn split_contract_version(value: &str) -> Option<(&str, &str)> {
    let (major, minor) = value.split_once('.')?;
    for part in [major, minor] {
        if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
    }
    if major.starts_with('0') || (minor.starts_with('0') && minor.len() > 1) {
        return None;
    }
    Some((major, minor))
}

/// Orders two version parts as NUMBERS without parsing them.
///
/// The pattern rules out a leading zero, so a longer run of digits is the
/// larger number and equal lengths compare lexicographically. Parsing instead
/// bounded the comparison at `u64`, and a major past that returned `None` -
/// which the caller reads as "does not match the pattern", so a version the
/// schema accepts was refused as a malformed envelope rather than as the higher
/// contract version it is (carve-rs#1965).
fn compare_version_part(left: &str, right: &str) -> Ordering {
    left.len().cmp(&right.len()).then_with(|| left.cmp(right))
}

/// An extension whose node types or fields the enveloped document uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AstEnvelopeExtension {
    /// Globally qualified, so two extensions cannot collide.
    pub id: String,
    /// The extension's own version, opaque to the envelope.
    pub version: Option<String>,
    /// Whether the document's MEANING depends on it. `None` means required: a
    /// reader that drops a required extension has rendered a different document.
    pub required: Option<bool>,
}

impl AstEnvelopeExtension {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            version: None,
            required: None,
        }
    }
}

/// What a producer declares about the tree it is wrapping.
#[derive(Debug, Clone, Default)]
pub struct AstEnvelopeOptions {
    /// `None` means the core vocabulary, which is the canonical spelling.
    pub vocabulary: Option<String>,
    pub extensions: Vec<AstEnvelopeExtension>,
}

/// What a reader can do, against which the payload's claims are checked.
#[derive(Debug, Clone, Default)]
pub struct AstEnvelopeReaderOptions {
    /// Extension ids this reader implements. Anything else marked required is refused.
    pub extensions: Vec<String>,
    /// Vocabularies this reader understands, beside the core one.
    pub vocabularies: Vec<String>,
}

/// Why an envelope could not be read.
///
/// A distinct type from [`AstJsonError`], which is about the tree INSIDE the
/// envelope: the whole point of §34 is that a reader can say which of the two it
/// is looking at, and which of the four envelope failures it hit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AstEnvelopeError {
    /// The envelope itself is not the shape the schema names.
    Shape(String),
    /// The payload announces a higher MAJOR contract version.
    ///
    /// §34(a) asks for a typed error naming both versions, and says why it is
    /// not a schema failure: the payload may be perfectly well-formed under a
    /// contract this build predates.
    Version { found: String, implemented: String },
    /// The payload marks an extension required that this reader does not
    /// implement (§34(c)).
    Extension(String),
    /// The payload names a vocabulary this build does not know. A foreign
    /// vocabulary is refused rather than walked, or its tree comes back as an
    /// unknown node type - the undifferentiated failure §34 exists to separate.
    Vocabulary(String),
    /// The envelope was readable and the tree inside it was not.
    Document(AstJsonError),
}

impl fmt::Display for AstEnvelopeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Shape(detail) => write!(
                f,
                "AST envelope is not the shape PART 12 §34 names: {detail}"
            ),
            Self::Version { found, implemented } => write!(
                f,
                "AST envelope announces contract version {found}; this build implements \
                 {implemented}. A higher major removes, renames or reinterprets, so the payload \
                 is refused rather than half-read (PART 12 §34(a))"
            ),
            Self::Extension(id) => write!(
                f,
                "AST envelope requires extension {id:?}, which this reader does not implement \
                 (PART 12 §34(c))"
            ),
            Self::Vocabulary(vocabulary) => write!(
                f,
                "AST envelope is written in vocabulary {vocabulary:?}, which this reader does \
                 not know (PART 12 §34)"
            ),
            Self::Document(error) => write!(f, "the enveloped AST could not be read: {error}"),
        }
    }
}

impl std::error::Error for AstEnvelopeError {}

fn json_string(value: &str) -> String {
    value_to_json(&Json::String(value.to_string()))
}

/// Wraps a document for a storage or process boundary.
///
/// The version emitted is always this build's own: re-emitting an ingested
/// `astVersion` would republish a claim the producer cannot keep, so §34 gives
/// no way to override it.
pub fn to_ast_envelope_json(
    doc: &Document,
    options: &AstEnvelopeOptions,
) -> Result<String, AstEnvelopeError> {
    let document = try_to_json(doc).map_err(AstEnvelopeError::Document)?;
    let mut out = format!("{{\"astVersion\":{}", json_string(AST_CONTRACT_VERSION));
    if let Some(vocabulary) = &options.vocabulary {
        out.push_str(&format!(",\"vocabulary\":{}", json_string(vocabulary)));
    }
    if !options.extensions.is_empty() {
        out.push_str(",\"extensions\":[");
        for (index, extension) in options.extensions.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str(&format!("{{\"id\":{}", json_string(&extension.id)));
            if let Some(version) = &extension.version {
                out.push_str(&format!(",\"version\":{}", json_string(version)));
            }
            if let Some(required) = extension.required {
                out.push_str(&format!(",\"required\":{required}"));
            }
            out.push('}');
        }
        out.push(']');
    }
    out.push_str(&format!(",\"document\":{document}}}"));
    Ok(out)
}

fn read_extension(entry: &Json, index: usize) -> Result<AstEnvelopeExtension, AstEnvelopeError> {
    let object = entry
        .as_object()
        .ok_or_else(|| AstEnvelopeError::Shape(format!("extension {index} is not an object")))?;
    for key in object.keys() {
        if !EXTENSION_FIELDS.contains(&key.as_str()) {
            return Err(AstEnvelopeError::Shape(format!(
                "extension {index} carries {key:?}, which the schema does not name"
            )));
        }
    }
    let id = match object.get("id") {
        Some(Json::String(id)) => id.clone(),
        _ => {
            return Err(AstEnvelopeError::Shape(format!(
                "extension {index} has no \"id\""
            )))
        }
    };
    let version = match object.get("version") {
        None | Some(Json::Null) => None,
        Some(Json::String(version)) => Some(version.clone()),
        Some(_) => {
            return Err(AstEnvelopeError::Shape(format!(
                "extension {id:?} has a non-string \"version\""
            )))
        }
    };
    let required = match object.get("required") {
        None | Some(Json::Null) => None,
        Some(Json::Bool(required)) => Some(*required),
        Some(_) => {
            return Err(AstEnvelopeError::Shape(format!(
                "extension {id:?} has a non-boolean \"required\""
            )))
        }
    };
    Ok(AstEnvelopeExtension {
        id,
        version,
        required,
    })
}

/// Reads an envelope and returns the document inside it.
///
/// The checks run widest-first, so the caller hears about the contract before
/// the tree: a payload from a newer major is refused whatever its tree looks
/// like, and the tree is only walked once the reader has agreed it can read this
/// contract, this vocabulary and these extensions at all.
pub fn from_ast_envelope_json(
    input: &str,
    options: &AstEnvelopeReaderOptions,
) -> Result<Document, AstEnvelopeError> {
    // A syntax error here is about the envelope payload, not the tree: nothing
    // has been read far enough to blame what is inside `document`.
    let value = parse_value(input).map_err(|error| AstEnvelopeError::Shape(error.to_string()))?;
    let object = value
        .as_object()
        .ok_or_else(|| AstEnvelopeError::Shape("the envelope is not an object".to_string()))?;
    // `additionalProperties: false` holds on the envelope as well as on every
    // node inside it: §34 closes it for the reason §11 closed the tree.
    for key in object.keys() {
        if !ENVELOPE_FIELDS.contains(&key.as_str()) {
            return Err(AstEnvelopeError::Shape(format!(
                "it carries {key:?}, which the schema does not name"
            )));
        }
    }

    let ast_version = match object.get("astVersion") {
        Some(Json::String(version)) => version.clone(),
        _ => {
            return Err(AstEnvelopeError::Shape(
                "\"astVersion\" is missing".to_string(),
            ))
        }
    };
    let Some((major, _)) = split_contract_version(&ast_version) else {
        return Err(AstEnvelopeError::Shape(format!(
            "\"astVersion\" is {ast_version:?}, which is not major.minor with no leading zero"
        )));
    };
    let Some(document) = object.get("document") else {
        return Err(AstEnvelopeError::Shape(
            "\"document\" is missing".to_string(),
        ));
    };

    let (our_major, _) = split_contract_version(AST_CONTRACT_VERSION)
        .expect("the build's own contract version matches the schema pattern");
    // A higher major only. A LOWER one cannot arrive while this build implements
    // major 1, which the schema's pattern makes the lowest there is.
    if compare_version_part(major, our_major) == Ordering::Greater {
        return Err(AstEnvelopeError::Version {
            found: ast_version,
            implemented: AST_CONTRACT_VERSION.to_string(),
        });
    }

    match object.get("vocabulary") {
        None | Some(Json::Null) => {}
        Some(Json::String(vocabulary)) => {
            if vocabulary != CORE_AST_VOCABULARY
                && !options.vocabularies.iter().any(|known| known == vocabulary)
            {
                return Err(AstEnvelopeError::Vocabulary(vocabulary.clone()));
            }
        }
        Some(_) => {
            return Err(AstEnvelopeError::Shape(
                "\"vocabulary\" is not a string".to_string(),
            ))
        }
    }

    match object.get("extensions") {
        None | Some(Json::Null) => {}
        Some(Json::Array(entries)) => {
            for (index, entry) in entries.iter().enumerate() {
                let extension = read_extension(entry, index)?;
                // Absent means true. An extension a reader may ignore without
                // misreading the document has to say so.
                if extension.required == Some(false) {
                    continue;
                }
                if !options.extensions.contains(&extension.id) {
                    return Err(AstEnvelopeError::Extension(extension.id));
                }
            }
        }
        Some(_) => {
            return Err(AstEnvelopeError::Shape(
                "\"extensions\" is not an array".to_string(),
            ))
        }
    }

    // A higher MINOR needs no gate of its own: §34(b) makes it acceptable
    // exactly where every required extension is implemented, which the loop
    // above is.
    from_json(&value_to_json(document)).map_err(AstEnvelopeError::Document)
}
