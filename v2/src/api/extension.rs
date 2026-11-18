//! PROVISIONAL: the extension host of `extensions.md` (C2-X). The types name
//! what a host registers (a declaration, renderers per render channel, a
//! document transform); this engine does not run host renderers or
//! transforms yet, so [`super::Carve::register_extension`] refuses with
//! [`super::Error::Unsupported`]. These types may change before 0.2 is final.

use super::{Error, Options, Tree};

/// An extension declaration (C2-A-017).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionDeclaration {
    /// The qualified name (C2-A-123).
    pub name: String,
    pub version: String,
    pub required: bool,
    pub slots: Vec<String>,
}

/// The render channel a renderer registers for (C2-X-052).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    /// `mode: interactive`.
    Interactive,
    /// `mode: static`: the build-time channel.
    BuildTime,
}

/// A host renderer of extension nodes (C2-X-033).
pub trait Renderer: Send + Sync {
    /// Writes the HTML of one extension node, given as its JSON value.
    ///
    /// # Errors
    ///
    /// Any error; the host records it as the renderer's failure.
    fn render(&self, node: &serde_json::Value, opts: &Options) -> Result<String, Error>;
}

/// A document transform (C2-X-053): takes the resolved render view and
/// returns a view.
pub trait DocumentTransform: Send + Sync {
    /// # Errors
    ///
    /// Any error; the host keeps the input view (C2-X-055).
    fn transform(&self, view: Tree) -> Result<Tree, Error>;
}

/// One registration: a declaration with its renderers and an optional transform.
pub struct Extension {
    pub declaration: ExtensionDeclaration,
    pub renderers: Vec<(Channel, Box<dyn Renderer>)>,
    pub transform: Option<Box<dyn DocumentTransform>>,
}

impl std::fmt::Debug for Extension {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Extension")
            .field("declaration", &self.declaration)
            .field("renderers", &self.renderers.len())
            .field("transform", &self.transform.is_some())
            .finish()
    }
}
