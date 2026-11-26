//! The package facts of the public library and the small helpers of the
//! parity ruling of 19 November (carve-js#5075): `headingId` and `positionOf`.

use super::{Error, HeadingIds, Input};

include!(concat!(env!("OUT_DIR"), "/package_info_gen.rs"));

/// The version of this package.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The specification an engine implements, and the commit of
/// markup-carve/carve it pins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpecVersion {
    pub version: &'static str,
    pub commit: &'static str,
}

/// The specification this engine implements.
pub const SPEC_VERSION: SpecVersion = SpecVersion {
    version: "0.2",
    commit: SPEC_COMMIT,
};

/// Node types of the tree schema: those a document holds as blocks, and
/// those a paragraph holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeTypes {
    pub block: &'static [&'static str],
    pub inline: &'static [&'static str],
}

/// The node types of the pinned `data/ast-schema.json`.
pub const NODE_TYPES: NodeTypes = NodeTypes {
    block: BLOCK_TYPES,
    inline: INLINE_TYPES,
};

/// The id the `headingIds` option generates for a heading with the plain
/// text `text` (C2-H-022, C2-H-143 to C2-H-148), without the suffix that
/// keeps ids unique in a document; `None` under `off`.
#[must_use]
pub fn heading_id(text: &str, mode: HeadingIds) -> Option<String> {
    (mode != HeadingIds::Off).then(|| crate::resolve::slug::slug(text, mode))
}

/// A line and a column of C2-T-019, both from 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

/// `positionOf` (C2-T-041): the line and column of a range that starts at
/// the UTF-8 byte offset `offset` of `input`.
///
/// # Errors
///
/// [`Error::Option`] naming `offset` for an offset past the end of the
/// source, or inside one code point or one replaced ill-formed sequence.
pub fn position_of<'a>(input: impl Into<Input<'a>>, offset: usize) -> Result<Position, Error> {
    let bytes = match input.into() {
        Input::Text(t) => t.as_bytes(),
        Input::Bytes(b) => b,
    };
    if offset > bytes.len() {
        return Err(Error::option(
            "offset",
            format!("{offset} is no offset into {} bytes", bytes.len()),
        ));
    }
    let (mut line, mut column, mut at) = (1, 1, 0);
    while at < offset {
        if bytes.get(at) == Some(&b'\n') {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
        at += unit_length(bytes, at);
    }
    if at != offset {
        return Err(Error::option(
            "offset",
            format!("{offset} is inside the bytes of one code point"),
        ));
    }
    Ok(Position { line, column })
}

/// The byte length of the code point at `at`, or of the ill-formed sequence
/// one U+FFFD replaces there (C2-B-218, the WHATWG UTF-8 decoder).
fn unit_length(bytes: &[u8], at: usize) -> usize {
    let lead = bytes.get(at).copied().unwrap_or(0);
    let (mut low, mut high, len) = match lead {
        0xc2..=0xdf => (0x80, 0xbf, 2),
        0xe0 => (0xa0, 0xbf, 3),
        0xed => (0x80, 0x9f, 3),
        0xe1..=0xef => (0x80, 0xbf, 3),
        0xf0 => (0x90, 0xbf, 4),
        0xf1..=0xf3 => (0x80, 0xbf, 4),
        0xf4 => (0x80, 0x8f, 4),
        _ => return 1,
    };
    let mut n = 1;
    while n < len {
        match bytes.get(at + n) {
            Some(&b) if (low..=high).contains(&b) => n += 1,
            _ => break,
        }
        low = 0x80;
        high = 0xbf;
    }
    n
}
