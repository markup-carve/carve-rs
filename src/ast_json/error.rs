use std::fmt;
use std::sync::Arc;

/// The stage that refused an AST JSON operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AstJsonErrorKind {
    /// The input is not valid JSON.
    Syntax,
    /// The reader or writer exceeded its nesting budget.
    DepthLimit,
    /// An object contains a field outside the AST schema.
    UnknownField,
    /// The decoded value or API tree violates the AST contract.
    InvalidAst,
}

/// A JSON syntax error or an AST contract refusal.
#[derive(Debug, Clone)]
pub struct AstJsonError {
    message: String,
    kind: AstJsonErrorKind,
    path: Option<String>,
    source: Option<Arc<serde_json::Error>>,
}

impl AstJsonError {
    pub(super) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: AstJsonErrorKind::InvalidAst,
            path: None,
            source: None,
        }
    }

    pub(super) fn depth(message: impl Into<String>) -> Self {
        Self {
            kind: AstJsonErrorKind::DepthLimit,
            ..Self::new(message)
        }
    }

    pub(super) fn unknown_field(message: impl Into<String>, path: String) -> Self {
        Self {
            kind: AstJsonErrorKind::UnknownField,
            path: Some(path),
            ..Self::new(message)
        }
    }

    pub(super) fn from_serde(error: serde_json::Error) -> Self {
        Self {
            message: error.to_string(),
            kind: AstJsonErrorKind::Syntax,
            path: None,
            source: Some(Arc::new(error)),
        }
    }

    pub fn kind(&self) -> AstJsonErrorKind {
        self.kind
    }

    /// The field path when the validator records one, using dot and index notation.
    pub fn path(&self) -> Option<&str> {
        self.path.as_deref()
    }

    /// The 1-based input line for a JSON syntax error.
    pub fn line(&self) -> Option<usize> {
        self.source.as_ref().map(|error| error.line())
    }

    /// The input column reported by serde_json for a syntax error.
    pub fn column(&self) -> Option<usize> {
        self.source.as_ref().map(|error| error.column())
    }
}

impl PartialEq for AstJsonError {
    fn eq(&self, other: &Self) -> bool {
        self.message == other.message && self.kind == other.kind && self.path == other.path
    }
}

impl Eq for AstJsonError {}

impl fmt::Display for AstJsonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for AstJsonError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_deref()
            .map(|error| error as &(dyn std::error::Error + 'static))
    }
}
