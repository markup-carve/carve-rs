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
pub mod code_callouts;
pub mod code_group;
pub mod color_swatch;
pub mod details;
pub mod external_links;
pub mod fenced_render;
pub mod glossary;
pub mod heading_level_shift;
pub mod heading_numbers;
pub mod heading_permalinks;
pub mod heading_reference;
pub mod img_fence;
pub mod index_terms;
pub mod list_table;
pub mod math_block;
pub mod spoiler;
pub mod svg_sanitize;
pub mod tab_normalize;
pub mod table_of_contents;
pub mod tabs;
pub mod wikilinks;

pub use autolink::{Autolink, AutolinkOptions};
pub use code_callouts::CodeCallouts;
pub use code_group::{CodeGroup, CodeGroupOptions};
pub use color_swatch::{ColorSwatch, SwatchPosition, SwatchShape};
pub use details::Details;
pub use external_links::{ExternalLinks, ExternalLinksOptions};
pub use fenced_render::{ContentMode, FencedRender, FencedRenderOptions};
pub use glossary::Glossary;
pub use heading_level_shift::{HeadingLevelShift, HeadingLevelShiftOptions};
pub use heading_numbers::{CrossrefStyle, HeadingNumbers, HeadingNumbersOptions};
pub use heading_permalinks::{HeadingPermalinks, HeadingPermalinksOptions};
pub use heading_reference::{HeadingReference, HeadingReferenceOptions};
pub use img_fence::ImgFence;
pub use index_terms::Index;
pub use list_table::ListTable;
pub use math_block::{MathBlock, MathBlockOptions};
pub use spoiler::Spoiler;
pub use svg_sanitize::{sanitize_svg, SanitizeResult, SanitizeSvgOptions};
pub use tab_normalize::TabNormalize;
pub use table_of_contents::{
    ListType, Position, TableOfContents, TableOfContentsOptions, TocPlacement,
};
pub use tabs::{Tabs, TabsMode, TabsOptions};
pub use wikilinks::{UrlGenerator, Wikilinks, WikilinksOptions};
