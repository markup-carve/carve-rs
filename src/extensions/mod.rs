//! Built-in opt-in Carve extensions, ported from the carve-js modules.
//!
//! Each extension is a struct implementing [`crate::CarveExtension`]. Register
//! one via [`crate::Options::with_extension`] and render through
//! [`crate::to_html_with_options`]:
//!
//! ```
//! use carve::{Autolink, Options};
//! let ext = Autolink::new();
//! let opts = Options::new().with_extension(&ext);
//! let html = carve::to_html_with_options("Visit https://example.com.", &opts);
//! assert!(html.contains("<a href=\"https://example.com\">"));
//! ```

pub mod autolink;
pub mod external_links;
pub mod heading_permalinks;
pub mod mermaid;
pub mod tab_normalize;
pub mod table_of_contents;
pub mod wikilinks;

pub use autolink::{Autolink, AutolinkOptions};
pub use external_links::{ExternalLinks, ExternalLinksOptions};
pub use heading_permalinks::{HeadingPermalinks, HeadingPermalinksOptions};
pub use mermaid::{Mermaid, MermaidOptions};
pub use tab_normalize::TabNormalize;
pub use table_of_contents::{ListType, Position, TableOfContents, TableOfContentsOptions};
pub use wikilinks::{UrlGenerator, Wikilinks, WikilinksOptions};
