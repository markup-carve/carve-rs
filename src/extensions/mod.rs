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
pub mod details;
pub mod external_links;
pub mod fenced_render;
pub mod glossary;
pub mod heading_permalinks;
pub mod index_terms;
pub mod list_table;
pub mod math_block;
pub mod spoiler;
pub mod tab_normalize;
pub mod table_of_contents;
pub mod wikilinks;

pub use autolink::{Autolink, AutolinkOptions};
pub use details::Details;
pub use external_links::{ExternalLinks, ExternalLinksOptions};
pub use fenced_render::{ContentMode, FencedRender, FencedRenderOptions};
pub use glossary::Glossary;
pub use heading_permalinks::{HeadingPermalinks, HeadingPermalinksOptions};
pub use index_terms::Index;
pub use list_table::ListTable;
pub use math_block::{MathBlock, MathBlockOptions};
pub use spoiler::Spoiler;
pub use tab_normalize::TabNormalize;
pub use table_of_contents::{ListType, Position, TableOfContents, TableOfContentsOptions};
pub use wikilinks::{UrlGenerator, Wikilinks, WikilinksOptions};
