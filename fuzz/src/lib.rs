//! Helpers shared by the fuzz targets.

use carve::api::{self, EncodeOptions, Options};
use serde_json::Value;

/// Inputs longer than this are skipped: the targets look for crashes and
/// gate failures, and the timing class has its own budget (`-timeout`).
pub const MAX_INPUT: usize = 64 * 1024;

/// The tree of `src` without positions, under the equality of C2-A-162.
/// `metadata.fields` is dropped: the frontmatter exception of C2-W-002.
#[must_use]
pub fn bare_tree(src: &str) -> Option<Value> {
    let tree = api::parse(src, &Options::default()).ok()?;
    let mut v: Value = serde_json::from_str(&api::encode_tree_with(
        &tree,
        EncodeOptions { positions: false },
    ))
    .ok()?;
    strip(&mut v);
    Some(v)
}

fn strip(v: &mut Value) {
    match v {
        Value::Object(m) => {
            m.remove("pos");
            m.remove("segments");
            m.remove("srcByteLength");
            m.remove("fields");
            m.values_mut().for_each(strip);
        }
        Value::Array(a) => a.iter_mut().for_each(strip),
        _ => {}
    }
}

/// Render options picked by one selector byte.
#[must_use]
pub fn options(sel: u8) -> Options {
    use carve::api::{HeadingIds, Mode, Profile, SoftBreak};
    let mut o = Options::default();
    o.profile = match sel & 3 {
        0 => Profile::Full,
        1 => Profile::Article,
        2 => Profile::Comment,
        _ => Profile::Minimal,
    };
    o.mode = if sel & 4 == 0 {
        Mode::Interactive
    } else {
        Mode::Static
    };
    o.soft_break = match (sel >> 3) & 3 {
        0 => SoftBreak::Newline,
        1 => SoftBreak::Space,
        _ => SoftBreak::Br,
    };
    o.smart_typography = sel & 0x20 != 0;
    o.sections = sel & 0x40 != 0;
    o.raw_html = sel & 0x80 != 0;
    o.source_lines = sel & 0x10 != 0;
    o.heading_ids = if sel & 0x08 != 0 {
        HeadingIds::AsciiLower
    } else {
        HeadingIds::KeepCase
    };
    o
}
