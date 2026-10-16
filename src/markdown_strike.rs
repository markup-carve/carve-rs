use std::borrow::Cow;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::OnceLock;

use pulldown_cmark::{BrokenLink, Event, LinkType, Options, Parser, Tag, TagEnd};
use regex::Regex;

const COMMENT: &str = "<!---->";
// Same-length stand-in for a tilde run that cannot pair; not a pulldown delimiter.
const INERT: u8 = b'%';

pub(crate) struct PreparedStrikes<'a> {
    pub text: Cow<'a, str>,
    source: &'a str,
    comments: Vec<usize>,
    inert: Vec<usize>,
    references: HashMap<(usize, usize), (String, String, bool)>,
}

impl PreparedStrikes<'_> {
    pub fn synthetic_comment(&self, event: &Event<'_>, range: &Range<usize>) -> bool {
        matches!(event, Event::InlineHtml(value) if value.as_ref() == COMMENT)
            && self.comments.binary_search(&range.start).is_ok()
    }

    /// Put back the tildes that `prepare` hid from the delimiter scan.
    pub fn restore<'e>(&self, event: Event<'e>, range: &Range<usize>) -> Event<'e> {
        let Event::Text(value) = &event else {
            return event;
        };
        let original = self.original_range(range.clone());
        let first = self.inert.partition_point(|&at| at < original.start);
        if self.inert.get(first).map_or(true, |&at| at >= original.end)
            || value.as_ref() != &self.text[range.clone()]
        {
            return event;
        }
        Event::Text(self.source[original].to_owned().into())
    }

    pub fn original_range(&self, range: Range<usize>) -> Range<usize> {
        let original = |at| {
            at - COMMENT.len()
                * self
                    .comments
                    .partition_point(|&start| start + COMMENT.len() <= at)
        };
        original(range.start)..original(range.end)
    }

    pub fn reference_has_title(&self, range: &Range<usize>) -> Option<bool> {
        self.references
            .get(&(range.start, range.end))
            .map(|(_, _, title)| *title)
    }

    pub fn reference(
        &self,
        link: BrokenLink<'_>,
    ) -> Option<(
        pulldown_cmark::CowStr<'static>,
        pulldown_cmark::CowStr<'static>,
    )> {
        let range = self.original_range(link.span);
        self.references
            .get(&(range.start, range.end))
            .map(|(destination, title, _)| (destination.clone().into(), title.clone().into()))
    }
}

fn punctuation(ch: char) -> bool {
    static PUNCTUATION: OnceLock<Regex> = OnceLock::new();
    let mut buffer = [0; 4];
    ch.is_ascii_punctuation()
        || PUNCTUATION
            .get_or_init(|| Regex::new(r"^\p{P}$").unwrap())
            .is_match(ch.encode_utf8(&mut buffer))
}

fn word_neighbor(value: Option<char>) -> bool {
    value.is_some_and(|ch| !ch.is_whitespace() && !punctuation(ch))
}

/// Return the literal delimiter when pulldown pairs underscores whose
/// flanks cmark-gfm rejects. Keep interior events, source coordinates, and
/// reference labels intact.
pub(crate) fn literal_underscore(
    source: &str,
    event: &Event<'_>,
    range: &Range<usize>,
) -> Option<Range<usize>> {
    let (count, opening) = match event {
        Event::Start(Tag::Emphasis) => (1, true),
        Event::End(TagEnd::Emphasis) => (1, false),
        Event::Start(Tag::Strong) => (2, true),
        Event::End(TagEnd::Strong) => (2, false),
        _ => return None,
    };
    let bytes = source.as_bytes();
    let start = range.start;
    let end = range.end;
    if !bytes[start..start + count].iter().all(|&byte| byte == b'_')
        || !bytes[end - count..end].iter().all(|&byte| byte == b'_')
    {
        return None;
    }
    let flanks = |at: usize| {
        let mut start = at;
        let mut end = at + count;
        while start > 0 && bytes[start - 1] == b'_' {
            start -= 1;
        }
        while bytes.get(end) == Some(&b'_') {
            end += 1;
        }
        if bytes.get(start.wrapping_sub(1)) != Some(&b'~') && bytes.get(end) != Some(&b'~') {
            return (true, true);
        }
        // cmark-gfm skips registered extension delimiters during this scan.
        let before = source[..start].chars().rev().find(|&ch| ch != '~');
        let after = source[end..].chars().find(|&ch| ch != '~');
        let before_space = before.map_or(true, char::is_whitespace);
        let after_space = after.map_or(true, char::is_whitespace);
        let before_punct = before.is_some_and(punctuation);
        let after_punct = after.is_some_and(punctuation);
        let left = !after_space && (!after_punct || before_space || before_punct);
        let right = !before_space && (!before_punct || after_space || after_punct);
        (
            left && (!right || before_punct),
            right && (!left || after_punct),
        )
    };
    if flanks(start).0 && flanks(end - count).1 {
        return None;
    }
    Some(if opening {
        start..start + count
    } else {
        end - count..end
    })
}

/// Supply intraword flanking to the Markdown reader without changing tilde
/// run lengths. Synthetic comments are removed from its events; offsets and
/// resolved reference targets retain their original coordinates.
pub(crate) fn prepare(source: &str, options: Options) -> PreparedStrikes<'_> {
    let mut result = PreparedStrikes {
        text: Cow::Borrowed(source),
        source,
        comments: Vec::new(),
        inert: Vec::new(),
        references: HashMap::new(),
    };
    let bytes = source.as_bytes();
    if !bytes.contains(&b'~') {
        return result;
    }
    let mut eligible = vec![false; source.len()];
    let mut scope = vec![0usize; source.len()];
    let mut scopes = vec![0usize];
    let mut next_scope = 1;
    let mut owners: Vec<Range<usize>> = Vec::new();
    let mut opaque = 0usize;
    let mut autolink = false;
    let mut parser = Parser::new_ext(source, options).into_offset_iter();
    while let Some((event, range)) = parser.next() {
        let inline = |tag: &TagEnd| {
            matches!(
                tag,
                TagEnd::Emphasis
                    | TagEnd::Strong
                    | TagEnd::Strikethrough
                    | TagEnd::Superscript
                    | TagEnd::Subscript
            )
        };
        match &event {
            Event::Start(tag) if !inline(&tag.to_end()) => {
                scopes.push(next_scope);
                next_scope += 1;
            }
            Event::End(tag) if !inline(tag) && scopes.len() > 1 => {
                scopes.pop();
            }
            _ => {}
        }
        let current = *scopes.last().unwrap();
        match &event {
            Event::Start(Tag::CodeBlock(_) | Tag::HtmlBlock | Tag::MetadataBlock(_)) => opaque += 1,
            Event::End(TagEnd::CodeBlock | TagEnd::HtmlBlock | TagEnd::MetadataBlock(_)) => {
                opaque = opaque.saturating_sub(1);
            }
            Event::End(TagEnd::Link) => autolink = false,
            _ => {}
        }
        match &event {
            Event::Start(
                Tag::Link {
                    link_type,
                    dest_url,
                    title,
                    id,
                    ..
                }
                | Tag::Image {
                    link_type,
                    dest_url,
                    title,
                    id,
                    ..
                },
            ) => {
                if let Some(previous) = owners
                    .last_mut()
                    .filter(|previous| previous.end >= range.start)
                {
                    previous.end = previous.end.max(range.end);
                } else {
                    owners.push(range.clone());
                }
                autolink = matches!(link_type, LinkType::Autolink | LinkType::Email);
                if matches!(
                    link_type,
                    LinkType::Reference | LinkType::Collapsed | LinkType::Shortcut
                ) {
                    let target = (
                        dest_url.to_string(),
                        title.to_string(),
                        parser
                            .reference_definitions()
                            .get(id)
                            .is_some_and(|definition| definition.title.is_some()),
                    );
                    result
                        .references
                        .insert((range.start, range.end), target.clone());
                    if bytes.get(range.start) == Some(&b'!') {
                        result
                            .references
                            .insert((range.start + 1, range.end), target);
                    }
                }
            }
            Event::Text(_) if opaque == 0 && !autolink => {
                eligible[range.clone()].fill(true);
                scope[range.clone()].fill(current);
            }
            Event::Start(Tag::Strikethrough) if opaque == 0 && !autolink => {
                let mut start = range.start;
                while start < range.end && bytes[start] == b'~' {
                    eligible[start] = true;
                    scope[start] = current;
                    start += 1;
                }
                let mut end = range.end;
                while end > range.start && bytes[end - 1] == b'~' {
                    end -= 1;
                    eligible[end] = true;
                    scope[end] = current;
                }
            }
            _ => {}
        }
    }
    for email in crate::markdown_gfm_autolinks::bare_links(
        source,
        crate::markdown_gfm_autolinks::Mode::Email,
    ) {
        let owner = owners.partition_point(|owner| owner.start <= email.start);
        if owner == 0 || owners[owner - 1].end <= email.start {
            eligible[email.start..email.end].fill(false);
        }
    }
    let runs = tilde_runs(source, &eligible, &scope);
    let mut insertions = Vec::new();
    for run in &runs {
        if !run.paired {
            result.inert.extend(run.start..run.end);
        } else if run.end == run.start + 1
            && word_neighbor(source[..run.start].chars().next_back())
            && word_neighbor(source[run.end..].chars().next())
        {
            insertions.extend([run.start, run.end]);
        }
    }
    if insertions.is_empty() && result.inert.is_empty() {
        return result;
    }
    let mut text = String::with_capacity(source.len() + insertions.len() * COMMENT.len());
    let mut from = 0;
    for at in insertions {
        text.push_str(&source[from..at]);
        result.comments.push(text.len());
        text.push_str(COMMENT);
        from = at;
    }
    text.push_str(&source[from..]);
    if !result.inert.is_empty() {
        let mut bytes = text.into_bytes();
        // Comment `i` sits before original offset `comments[i] - i * len`.
        let mut before = 0;
        for &at in &result.inert {
            while result
                .comments
                .get(before)
                .is_some_and(|&start| start - before * COMMENT.len() <= at)
            {
                before += 1;
            }
            bytes[at + before * COMMENT.len()] = INERT;
        }
        text = String::from_utf8(bytes).expect("an ASCII byte replaced by an ASCII byte");
    }
    result.text = Cow::Owned(text);
    result
}

struct TildeRun {
    start: usize,
    end: usize,
    paired: bool,
}

/// Pair GFM tilde runs the way markdown-it and carve-js do: a closer skips an
/// opener of another length instead of failing. pulldown-cmark lets that
/// failure block every later tilde closer, so a lone `~` inside `~~...~~`
/// cost the whole strike.
fn tilde_runs(source: &str, eligible: &[bool], scope: &[usize]) -> Vec<TildeRun> {
    let bytes = source.as_bytes();
    let mut runs = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] != b'~' {
            at += 1;
            continue;
        }
        let end = at + bytes[at..].iter().take_while(|&&byte| byte == b'~').count();
        let escaped = bytes[..at]
            .iter()
            .rev()
            .take_while(|&&byte| byte == b'\\')
            .count()
            % 2
            == 1;
        if end - at <= 2
            && !escaped
            && eligible[at..end].iter().all(|&ok| ok)
            && scope[at..end].iter().all(|&id| id == scope[at])
        {
            runs.push(TildeRun {
                start: at,
                end,
                paired: false,
            });
        }
        at = end;
    }
    let whitespace = |ch: Option<char>| ch.map_or(true, char::is_whitespace);
    let punct = |ch: Option<char>| ch.is_some_and(punctuation);
    // Per scope: the open runs, and per length the indices into that stack.
    let mut stacks: HashMap<usize, (Vec<usize>, [Vec<usize>; 2])> = HashMap::new();
    for index in 0..runs.len() {
        let (start, end) = (runs[index].start, runs[index].end);
        let before = source[..start].chars().next_back();
        let after = source[end..].chars().next();
        let left = !whitespace(after) && (!punct(after) || whitespace(before) || punct(before));
        let right = !whitespace(before) && (!punct(before) || whitespace(after) || punct(after));
        let (stack, by_length) = stacks.entry(scope[start]).or_default();
        let length = end - start - 1;
        if right {
            if let Some(&position) = by_length[length].last() {
                let opener = stack[position];
                runs[opener].paired = true;
                runs[index].paired = true;
                stack.truncate(position);
                for list in by_length.iter_mut() {
                    while list.last().is_some_and(|&p| p >= position) {
                        list.pop();
                    }
                }
                continue;
            }
        }
        if left {
            by_length[length].push(stack.len());
            stack.push(index);
        }
    }
    runs
}
