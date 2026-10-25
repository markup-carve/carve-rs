//! Experimental physical-line ownership pass for the block parser.
//! Admitted lines retain byte ranges into the original source.

use super::{detect_list_marker_full, strip_blockquote_prefix};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FrameKind {
    Root,
    Quote,
    ListItem,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct FrameRecord {
    pub(super) kind: FrameKind,
    pub(super) parent: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct OwnedLine {
    pub(super) source_start: usize,
    pub(super) source_end: usize,
    pub(super) content_start: usize,
    pub(super) frame_id: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct OwnedSource {
    pub(super) frames: Vec<FrameRecord>,
    pub(super) lines: Vec<OwnedLine>,
}

#[derive(Clone, Copy)]
struct OpenFrame {
    id: usize,
    kind: FrameKind,
    content_column: usize,
}

/// This accepts a narrow list and quote subset. It declines lazy lines, blank
/// lines in containers, root indentation, and unmodeled block openers. It is
/// isolated from public parsing until AST parity and route CPU checks pass.
#[allow(dead_code)]
pub(super) fn scan(source: &str) -> Option<OwnedSource> {
    let mut result = OwnedSource {
        frames: vec![FrameRecord {
            kind: FrameKind::Root,
            parent: None,
        }],
        lines: Vec::new(),
    };
    let mut stack = vec![OpenFrame {
        id: 0,
        kind: FrameKind::Root,
        content_column: 0,
    }];
    let mut offset = 0;
    let mut previous_blank = true;
    for physical in source.split_inclusive('\n') {
        let source_start = offset;
        offset += physical.len();
        let line = physical.trim_end_matches(['\r', '\n']);
        if line.contains('\t') {
            return None;
        }
        if line.trim().is_empty() {
            if stack.len() != 1 {
                return None;
            }
            result.lines.push(OwnedLine {
                source_start,
                source_end: offset,
                content_start: source_start,
                frame_id: 0,
            });
            previous_blank = true;
            continue;
        }
        if detect_list_marker_full(line).is_some_and(|marker| marker.indent == 0) {
            if stack
                .get(1)
                .is_some_and(|frame| frame.kind == FrameKind::Quote)
            {
                return None;
            }
            stack.truncate(1);
        }
        let mut remaining = line;
        let mut consumed = 0;
        let mut depth = 1;
        while let Some(frame) = stack.get(depth).copied() {
            match frame.kind {
                FrameKind::Quote => {
                    let rest = strip_blockquote_prefix(remaining)?;
                    consumed += remaining.len() - rest.len();
                    remaining = rest;
                }
                FrameKind::ListItem => {
                    if detect_list_marker_full(remaining).is_some_and(|marker| marker.indent == 0) {
                        break;
                    }
                    let prefix = remaining.bytes().take_while(|byte| *byte == b' ').count();
                    if prefix < frame.content_column {
                        return None;
                    }
                    remaining = &remaining[frame.content_column..];
                    consumed += frame.content_column;
                }
                FrameKind::Root => unreachable!(),
            }
            depth += 1;
        }
        stack.truncate(depth);
        if stack.len() == 1 && remaining.starts_with(' ') {
            return None;
        }
        loop {
            let (kind, rest, width) = if let Some(rest) = strip_blockquote_prefix(remaining) {
                (FrameKind::Quote, rest, remaining.len() - rest.len())
            } else if let Some(marker) = detect_list_marker_full(remaining) {
                let width = marker.content.as_ptr() as usize - remaining.as_ptr() as usize;
                (FrameKind::ListItem, marker.content, width)
            } else {
                break;
            };
            if !previous_blank
                && result
                    .lines
                    .last()
                    .is_some_and(|last| last.frame_id == stack.last().unwrap().id)
            {
                return None;
            }
            consumed += width;
            remaining = rest;
            let id = result.frames.len();
            result.frames.push(FrameRecord {
                kind,
                parent: Some(stack.last().unwrap().id),
            });
            stack.push(OpenFrame {
                id,
                kind,
                content_column: width,
            });
        }
        if remaining.starts_with(['#', ':', '|', '%', '+', '{', '^', '>', '`', '~', '['])
            || detect_list_marker_full(remaining).is_some()
            || remaining.starts_with(' ')
        {
            return None;
        }
        result.lines.push(OwnedLine {
            source_start,
            source_end: offset,
            content_start: source_start + consumed,
            frame_id: stack.last().unwrap().id,
        });
        previous_blank = false;
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::BlockNode;

    #[test]
    fn records_sibling_frames_and_original_byte_ranges() {
        let source = "> - café  \n> - next\n";
        let result = scan(source).unwrap();
        assert_eq!(result.lines.len(), 2);
        let first = result.lines[0];
        let second = result.lines[1];
        assert_ne!(first.frame_id, second.frame_id);
        assert_eq!(&source[first.content_start..first.source_end], "café  \n");
        assert_eq!(second.source_start, "> - café  \n".len());
        assert_eq!(
            result.frames[first.frame_id].parent,
            result.frames[second.frame_id].parent
        );
    }

    #[test]
    fn records_nested_list_and_quote_frames() {
        let result = scan("- > quoted\n  > next\n- sibling\n").unwrap();
        let ids: Vec<_> = result.lines.iter().map(|line| line.frame_id).collect();
        assert_eq!(result.frames[ids[0]].kind, FrameKind::Quote);
        assert_eq!(ids[0], ids[1]);
        assert_eq!(result.frames[ids[0]].parent, Some(1));
        assert_ne!(ids[0], ids[2]);
    }

    #[test]
    fn declines_lines_without_a_proven_owner() {
        for source in [
            "- item\n\n  continuation\n",
            "- item\n   \n  continuation\n",
            "> - item\nlazy\n",
            "- item\n> quote\n",
            "- item\n  ```\n",
            "> ~~~\n",
            "> # heading\n",
            "   # heading\n",
            "a\n- b\n",
            "> a\n> - b\n",
            "> a\n- b\n",
            "> - a\n- b\n",
        ] {
            assert!(scan(source).is_none(), "{source:?}");
        }
    }

    #[test]
    fn admitted_siblings_match_the_public_ast_structure() {
        let plain = "- first\n- second\n";
        let owned = scan(plain).unwrap();
        let ast = crate::parse(plain);
        let BlockNode::List(list) = &ast.children[0] else {
            panic!("expected a root list");
        };
        assert_eq!(list.items.len(), 2);
        assert_eq!(owned.lines.len(), list.items.len());
        assert_ne!(owned.lines[0].frame_id, owned.lines[1].frame_id);

        let quoted = "> - first\n> - second\n";
        let owned = scan(quoted).unwrap();
        let ast = crate::parse(quoted);
        let BlockNode::BlockQuote(quote) = &ast.children[0] else {
            panic!("expected a root quote");
        };
        let BlockNode::List(list) = &quote.children[0] else {
            panic!("expected a list in the quote");
        };
        assert_eq!(list.items.len(), 2);
        let first_parent = owned.frames[owned.lines[0].frame_id].parent;
        let second_parent = owned.frames[owned.lines[1].frame_id].parent;
        assert_eq!(first_parent, second_parent);
        assert_eq!(owned.frames[first_parent.unwrap()].kind, FrameKind::Quote);
    }
}
