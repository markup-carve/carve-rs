//! Experimental physical-line ownership pass for the block parser.
//! Admitted lines retain byte ranges into the original source.

use super::{detect_list_marker_full, parse_inline_with_options, strip_blockquote_prefix, Options};
use crate::ast::{BlockNode, BlockQuote, Document, List, ListItem, Paragraph};
use std::collections::BTreeMap;

#[cfg(all(test, target_os = "linux", target_pointer_width = "64"))]
fn thread_cpu_ns() -> i64 {
    #[repr(C)]
    struct Timespec {
        seconds: i64,
        nanoseconds: i64,
    }
    extern "C" {
        fn clock_gettime(clock: i32, time: *mut Timespec) -> i32;
    }
    let mut time = Timespec {
        seconds: 0,
        nanoseconds: 0,
    };
    assert_eq!(unsafe { clock_gettime(3, &mut time) }, 0);
    time.seconds * 1_000_000_000 + time.nanoseconds
}

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
    pub(super) bullet: Option<char>,
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
            bullet: None,
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
            let (kind, rest, width, bullet) = if let Some(rest) = strip_blockquote_prefix(remaining)
            {
                (FrameKind::Quote, rest, remaining.len() - rest.len(), None)
            } else if let Some(marker) = detect_list_marker_full(remaining) {
                let width = marker.content.as_ptr() as usize - remaining.as_ptr() as usize;
                let bullet =
                    (!marker.ordered && marker.checked.is_none() && marker.attrs.is_none())
                        .then(|| remaining.trim_start().chars().next())
                        .flatten();
                (FrameKind::ListItem, marker.content, width, bullet)
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
                bullet,
            });
            stack.push(OpenFrame {
                id,
                kind,
                content_column: width,
            });
            if stack.len() > 64 {
                return None;
            }
        }
        if remaining.starts_with(['#', ':', '|', '%', '+', '{', '^', '>', '`', '~', '['])
            || detect_list_marker_full(remaining).is_some()
            || remaining.starts_with(' ')
        {
            return None;
        }
        if remaining.trim().is_empty() && stack.len() > 1 {
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

enum Entry<'a> {
    Line(&'a str),
    Blank,
    Child(usize),
}

/// Build an unpositioned public tree for the scan's plain bullet/quote slice.
/// The public parser still handles all production calls. This gives the
/// prototype an exact AST and HTML comparison target before it is widened.
#[allow(dead_code)]
pub(super) fn parse_plain(source: &str) -> Option<Document> {
    let owned = scan(source)?;
    for line in &owned.lines {
        let text = source[line.content_start..line.source_end].trim_end_matches(['\r', '\n']);
        if text.trim().len() >= 3 && text.trim().bytes().all(|byte| byte == b'-') {
            return None;
        }
        if text.trim_end() != text
            || text.chars().any(|character| {
                !(character.is_alphanumeric()
                    || character == ' '
                    || ".,!?;:'\"()-&".contains(character))
            })
        {
            return None;
        }
    }
    if owned
        .frames
        .iter()
        .any(|frame| frame.kind == FrameKind::ListItem && !matches!(frame.bullet, Some('-' | '*')))
    {
        return None;
    }
    let mut entries: Vec<Vec<Entry<'_>>> = (0..owned.frames.len()).map(|_| Vec::new()).collect();
    let mut known_frames = 1;
    for line in &owned.lines {
        for id in known_frames..=line.frame_id {
            let parent = owned.frames[id].parent?;
            entries[parent].push(Entry::Child(id));
        }
        known_frames = known_frames.max(line.frame_id + 1);
        let text = source[line.content_start..line.source_end].trim_end_matches(['\r', '\n']);
        if text.trim().is_empty() {
            entries[line.frame_id].push(Entry::Blank);
        } else {
            entries[line.frame_id].push(Entry::Line(text));
        }
    }
    let children = build_blocks(0, &owned.frames, &entries, &Options::default())?;
    Some(Document {
        frontmatter: BTreeMap::new(),
        frontmatter_raw: None,
        footnote_defs: BTreeMap::new(),
        footnote_def_pos: BTreeMap::new(),
        children,
        source_len: source.len(),
        ingest_payload_len: 0,
    })
}

fn build_blocks(
    frame: usize,
    frames: &[FrameRecord],
    entries: &[Vec<Entry<'_>>],
    options: &Options<'_>,
) -> Option<Vec<BlockNode>> {
    let mut blocks = Vec::new();
    let mut paragraph = Vec::new();
    let mut items = Vec::new();
    let mut bullet = None;
    for entry in &entries[frame] {
        match entry {
            Entry::Line(text) => {
                flush_list(&mut blocks, &mut items, &mut bullet);
                paragraph.push(*text);
            }
            Entry::Blank => {
                flush_paragraph(&mut blocks, &mut paragraph, options);
                flush_list(&mut blocks, &mut items, &mut bullet);
            }
            Entry::Child(id) => {
                flush_paragraph(&mut blocks, &mut paragraph, options);
                let child = &frames[*id];
                match child.kind {
                    FrameKind::Quote => {
                        flush_list(&mut blocks, &mut items, &mut bullet);
                        blocks.push(BlockNode::BlockQuote(BlockQuote {
                            attrs: None,
                            children: build_blocks(*id, frames, entries, options)?,
                            fenced: false,
                            pos: None,
                        }));
                    }
                    FrameKind::ListItem => {
                        if bullet.is_some() && bullet != child.bullet {
                            flush_list(&mut blocks, &mut items, &mut bullet);
                        }
                        bullet = child.bullet;
                        items.push(ListItem {
                            attrs: None,
                            checked: None,
                            task_state: None,
                            children: build_blocks(*id, frames, entries, options)?,
                            pos: None,
                        });
                    }
                    FrameKind::Root => return None,
                }
            }
        }
    }
    flush_paragraph(&mut blocks, &mut paragraph, options);
    flush_list(&mut blocks, &mut items, &mut bullet);
    Some(blocks)
}

fn flush_paragraph(blocks: &mut Vec<BlockNode>, lines: &mut Vec<&str>, options: &Options<'_>) {
    if lines.is_empty() {
        return;
    }
    let text = lines.join("\n");
    blocks.push(BlockNode::Paragraph(Paragraph {
        attrs: None,
        children: parse_inline_with_options(&text, options),
        at_content_column: true,
        block_image: false,
        pos: None,
    }));
    lines.clear();
}

fn flush_list(blocks: &mut Vec<BlockNode>, items: &mut Vec<ListItem>, bullet: &mut Option<char>) {
    let Some(marker) = bullet.take() else {
        return;
    };
    blocks.push(BlockNode::List(List {
        attrs: None,
        ordered: false,
        start: None,
        ol_type: None,
        bare_marker: false,
        delim: None,
        bullet_char: (marker != '-').then_some(marker),
        tight: true,
        items: std::mem::take(items),
        pos: None,
    }));
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
            "> - a\n>   \n> - b\n",
            "> - a\n>   \n>   b\n",
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

    #[test]
    fn plain_builder_matches_public_ast_and_html() {
        for source in [
            "plain text\n",
            "one\ntwo\n",
            "- first\n- second\n",
            "> quoted\n> next\n",
            "> - first\n> - second\n",
            "- > quoted\n  > next\n- sibling\n",
            "- first\r\n- second\r\n",
        ] {
            let candidate = parse_plain(source).unwrap();
            let established = crate::parse(source);
            assert_eq!(candidate, established, "AST for {source:?}");
            assert_eq!(
                crate::render_html(&candidate).unwrap(),
                crate::to_html(source),
                "HTML for {source:?}"
            );
        }
    }

    #[test]
    fn admitted_spec_corpus_matches_public_ast() {
        let corpus = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/spec/tests/corpus");
        let mut checked = 0;
        let mut mismatches = Vec::new();
        for entry in std::fs::read_dir(corpus).unwrap() {
            let path = entry.unwrap().path();
            if path
                .extension()
                .map_or(true, |extension| extension != "crv")
            {
                continue;
            }
            let source = std::fs::read_to_string(&path).unwrap();
            let Some(candidate) = parse_plain(&source) else {
                continue;
            };
            checked += 1;
            if candidate != crate::parse(&source)
                || crate::render_html(&candidate).unwrap() != crate::to_html(&source)
            {
                mismatches.push(path.file_name().unwrap().to_string_lossy().into_owned());
            }
        }
        eprintln!(
            "admitted {checked} corpus files; {} mismatches",
            mismatches.len()
        );
        assert!(
            mismatches.is_empty(),
            "first mismatches: {:?}",
            &mismatches[..mismatches.len().min(8)]
        );
    }

    #[cfg(all(target_os = "linux", target_pointer_width = "64"))]
    #[test]
    #[ignore = "paired thread CPU screen for the admitted plain subset"]
    fn paired_complete_ast_html_cpu() {
        use std::hint::black_box;

        fn run(source: &str, candidate: bool) {
            let doc = if candidate {
                parse_plain(source).unwrap()
            } else {
                crate::parse(source)
            };
            black_box(crate::render_html(&doc).unwrap());
        }
        for (name, source) in [
            ("prose", "plain text\n\n".repeat(64)),
            ("list", "- plain item\n".repeat(64)),
            ("quote", "> plain text\n".repeat(64)),
        ] {
            let candidate = parse_plain(&source).unwrap();
            let established = crate::parse(&source);
            assert_eq!(candidate, established, "{name} AST");
            assert_eq!(
                crate::render_html(&candidate),
                crate::render_html(&established)
            );
            let mut ratios = Vec::new();
            for round in 0..24 {
                let measure = |candidate| {
                    let start = thread_cpu_ns();
                    for _ in 0..100 {
                        run(&source, candidate);
                    }
                    thread_cpu_ns() - start
                };
                let (candidate_ns, baseline_ns) = if round % 2 == 0 {
                    (measure(true), measure(false))
                } else {
                    let baseline = measure(false);
                    (measure(true), baseline)
                };
                ratios.push(candidate_ns as f64 / baseline_ns as f64);
            }
            ratios.sort_by(f64::total_cmp);
            eprintln!(
                "{name}: candidate/baseline median CPU ratio {:.3}",
                (ratios[11] + ratios[12]) / 2.0
            );
        }
    }

    #[cfg(all(target_os = "linux", target_pointer_width = "64"))]
    #[test]
    #[ignore = "local authored-document screen; set CARVE_DOCS_ROOT"]
    fn paired_authored_fallback_cpu() {
        use std::hint::black_box;
        use std::path::Path;

        let root = std::env::var("CARVE_DOCS_ROOT").expect("set CARVE_DOCS_ROOT");
        for (name, relative) in [
            ("terms", "terms.md"),
            ("parsing-ast", "case-study/parsing-ast.md"),
            ("spec-history", "spec-history.md"),
        ] {
            let path = Path::new(&root).join(relative);
            let source = std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
            let admitted = parse_plain(&source).is_some();
            assert!(!admitted, "{name} no longer tests the fallback route");
            let established = crate::parse(&source);
            let candidate = parse_plain(&source).unwrap_or_else(|| crate::parse(&source));
            assert_eq!(candidate, established);
            let mut ratios = Vec::new();
            let mut deltas = Vec::new();
            for round in 0..24 {
                let measure = |candidate: bool| {
                    let start = thread_cpu_ns();
                    for _ in 0..40 {
                        let doc = if candidate {
                            parse_plain(&source).unwrap_or_else(|| crate::parse(&source))
                        } else {
                            crate::parse(&source)
                        };
                        black_box(crate::render_html(&doc).unwrap());
                    }
                    thread_cpu_ns() - start
                };
                let (candidate_ns, baseline_ns) = if round % 2 == 0 {
                    (measure(true), measure(false))
                } else {
                    let baseline = measure(false);
                    (measure(true), baseline)
                };
                ratios.push(candidate_ns as f64 / baseline_ns as f64);
                deltas.push((candidate_ns - baseline_ns) as f64 / 40.0);
            }
            ratios.sort_by(f64::total_cmp);
            deltas.sort_by(f64::total_cmp);
            eprintln!(
                "{name}: {} bytes at {}, fallback/baseline CPU {:.3}, delta {:.0} ns/op",
                source.len(),
                path.display(),
                (ratios[11] + ratios[12]) / 2.0,
                (deltas[11] + deltas[12]) / 2.0
            );
        }
    }
}
