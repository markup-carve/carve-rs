use std::collections::{BTreeMap, HashMap};

use crate::ast::{BlockNode, Comment, Document, InlineNode};
use crate::render_depth::{push_block_children, push_inline_children};

use super::LintWarning;

enum Scope {
    Line,
    NextLine,
    Document(bool),
}

struct Directive {
    line: usize,
    offset: usize,
    rules: Vec<String>,
    scope: Scope,
}

impl Directive {
    fn matches(&self, rule: &str) -> bool {
        self.rules.is_empty() || self.rules.iter().any(|id| id == rule)
    }
}

fn parse_comment(
    comment: &Comment,
    source: &str,
    to_byte: &dyn Fn(usize) -> usize,
) -> Option<Directive> {
    if comment.block || comment.delimited {
        return None;
    }
    let pos = comment.pos.as_ref()?;
    let offset = to_byte(pos.start_offset);
    if !source
        .as_bytes()
        .get(offset + 2..)?
        .starts_with(b" carve-lint-")
    {
        return None;
    }
    let body = comment.content.strip_prefix("carve-lint-")?;
    let (name, ids) = body.split_once(' ').unwrap_or((body, ""));
    let scope = match name {
        "disable-line" => Scope::Line,
        "disable-next-line" => Scope::NextLine,
        "disable" => Scope::Document(true),
        "enable" => Scope::Document(false),
        _ => return None,
    };
    let rules = ids
        .split([' ', ','])
        .filter(|id| !id.is_empty())
        .map(|id| {
            if id == "a11y/heading-jump" {
                "heading-level-skip".to_owned()
            } else {
                id.to_owned()
            }
        })
        .collect();
    Some(Directive {
        line: pos.start_line,
        offset,
        rules,
        scope,
    })
}

pub(super) fn apply(
    source: &str,
    doc: &Document,
    to_byte: &dyn Fn(usize) -> usize,
    warnings: &mut Vec<LintWarning>,
) {
    let mut blocks: Vec<_> = doc.children.iter().map(|block| (block, 0)).collect();
    for body in doc.footnote_defs.values() {
        blocks.extend(body.iter().map(|block| (block, 0)));
    }
    let mut inlines = Vec::new();
    let mut directives = Vec::new();
    while let Some((block, depth)) = blocks.pop() {
        if let BlockNode::Comment(comment) = block {
            if let Some(directive) = parse_comment(comment, source, to_byte) {
                directives.push(directive);
            }
        }
        push_block_children(block, depth, &mut blocks, &mut inlines);
    }
    while let Some((inline, depth)) = inlines.pop() {
        if let InlineNode::Comment(comment) = inline {
            if let Some(directive) = parse_comment(comment, source, to_byte) {
                directives.push(directive);
            }
        }
        push_inline_children(inline, depth, &mut inlines);
    }
    directives.sort_by_key(|directive| (directive.line, directive.offset));
    let mut line_scopes: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (index, directive) in directives.iter().enumerate() {
        let line = match directive.scope {
            Scope::Line => Some(directive.line),
            Scope::NextLine => Some(directive.line + 1),
            Scope::Document(_) => None,
        };
        if let Some(line) = line {
            line_scopes.entry(line).or_default().push(index);
        }
    }
    let mut pending = std::mem::take(warnings);
    pending.sort_by_key(|finding| (finding.line, finding.start));
    let mut cursor = 0;
    let mut all_rules: Option<(usize, bool)> = None;
    let mut named_rules: HashMap<String, (usize, bool)> = HashMap::new();
    for finding in pending {
        while cursor < directives.len() && directives[cursor].line <= finding.line {
            let directive = &directives[cursor];
            if let Scope::Document(disabled) = directive.scope {
                if directive.rules.is_empty() {
                    all_rules = Some((cursor, disabled));
                } else {
                    for rule in &directive.rules {
                        named_rules.insert(rule.clone(), (cursor, disabled));
                    }
                }
            }
            cursor += 1;
        }
        let line_disabled = line_scopes.get(&finding.line).is_some_and(|indexes| {
            indexes
                .iter()
                .any(|&index| directives[index].matches(finding.rule))
        });
        let document_disabled = [all_rules, named_rules.get(finding.rule).copied()]
            .into_iter()
            .flatten()
            .max_by_key(|(index, _)| *index)
            .is_some_and(|(_, disabled)| disabled);
        if !line_disabled && !document_disabled {
            warnings.push(finding);
        }
    }
}
