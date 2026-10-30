use super::inline_resolution::mask_literal_directive_backticks;
use super::substitution_scanner::SubstitutionScanner;
use super::{
    delim_brace_slot, find_seq, raw_inline_format_end, read_attrs_at, scanned_autolink_end,
    skip_code_span, trailing_comment_end, DELIM_BRACE_SLOTS, MAX_NESTING_DEPTH,
};
use std::collections::HashMap;
use std::rc::Rc;

const RAW_CLOSE_FLAG: usize = 1 << (usize::BITS - 1);

#[cfg(test)]
thread_local! {
    static NATIVE_PAIR_SCAN_BYTES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[derive(Clone)]
pub(super) struct NativeBracedView {
    data: Rc<NativeBracedData>,
    offset: usize,
    length: usize,
}

struct NativeBracedData {
    ends: HashMap<usize, (usize, Option<usize>)>,
    arrows: HashMap<usize, usize>,
    attributes: HashMap<usize, usize>,
    bare: HashMap<usize, (usize, usize)>,
    bare_at_start: HashMap<usize, (usize, usize)>,
}

pub(super) struct CodeSpanIndex {
    runs: Vec<(usize, usize)>,
    ends_by_width: HashMap<usize, Vec<usize>>,
}

impl CodeSpanIndex {
    pub(super) fn new(bytes: &[u8]) -> Self {
        let mut runs = Vec::new();
        let mut ends_by_width: HashMap<usize, Vec<usize>> = HashMap::new();
        let mut at = 0;
        while at < bytes.len() {
            if bytes[at] != b'`' {
                at += 1;
                continue;
            }
            let start = at;
            while at < bytes.len() && bytes[at] == b'`' {
                at += 1;
            }
            runs.push((start, at));
            ends_by_width.entry(at - start).or_default().push(at);
        }
        Self {
            runs,
            ends_by_width,
        }
    }

    pub(super) fn end(&self, pos: usize) -> Option<usize> {
        let run = self
            .runs
            .partition_point(|&(start, _)| start <= pos)
            .checked_sub(1)?;
        let end = self.runs[run].1;
        if pos >= end {
            return None;
        }
        let width = end - pos;
        let closers = self.ends_by_width.get(&width)?;
        closers
            .get(closers.partition_point(|&close| close - width < end))
            .copied()
    }
}

/// Scan results depend on delimiter, nesting depth and scan position. Cache only
/// positions the scanner visits; bytes inside an opaque span start another scan.
#[derive(Default)]
pub(super) struct BracedClosers {
    /// The text with its literal directives' backticks masked, built once.
    code_view: Option<Rc<[u8]>>,
    queries: usize,
    last_verbatim_start: Option<usize>,
    opaque: HashMap<usize, usize>,
    /// `[^` references an image opener holds in `opaque`; the editorial and
    /// substitution scans read the reference first (`edScan`, `edSubAtom`).
    note_refs: HashMap<usize, usize>,
    states: HashMap<(u8, usize, usize), Option<usize>>,
    substitutions: SubstitutionScanner,
    code_ends: Option<CodeSpanIndex>,
    raw_closers: Option<Box<[Vec<usize>; DELIM_BRACE_SLOTS]>>,
    depth_thresholds: Option<Box<[Option<usize>; MAX_NESTING_DEPTH + 1]>>,
    last_closers: Option<Box<[Option<usize>; DELIM_BRACE_SLOTS]>>,
    pending: usize,
    dense: Option<Box<DenseBracedClosers>>,
    emphasis_pairs: Option<Box<DenseBracedClosers>>,
    substitution_hosts: Option<HashMap<usize, usize>>,
    native_opaque: Option<HashMap<usize, usize>>,
    line_comment_ends: HashMap<usize, usize>,
    attribute_tails: HashMap<usize, usize>,
    has_attribute_candidates: Option<bool>,
    comment_closes: Option<Vec<usize>>,
    bare_attribute_pairs: HashMap<usize, (usize, usize)>,
    bare_attribute_pairs_at_start: HashMap<usize, (usize, usize)>,
    native_view: Option<NativeBracedView>,
    exported_native: Option<Rc<NativeBracedData>>,
    #[cfg(test)]
    steps: usize,
}

impl BracedClosers {
    pub(super) fn inherit(&mut self, view: Option<NativeBracedView>) {
        self.native_view = view;
    }

    pub(super) fn view(&mut self, bytes: &[u8], start: usize, length: usize) -> NativeBracedView {
        if let Some(view) = &self.native_view {
            return NativeBracedView {
                data: Rc::clone(&view.data),
                offset: view.offset + start,
                length,
            };
        }
        if self.exported_native.is_none() {
            if self.emphasis_pairs.is_none() {
                self.build_emphasis_pairs(bytes);
            }
            let mut ends = HashMap::new();
            let points = self.emphasis_pairs.as_ref().unwrap().points.clone();
            let outer_verbatim = self.last_verbatim_start;
            for open in points {
                if bytes.get(open) != Some(&b'{') {
                    continue;
                }
                let Some(&marker) = bytes.get(open + 1) else {
                    continue;
                };
                if !matches!(marker, b'/' | b'*' | b'_' | b'^' | b',' | b'~' | b'=') {
                    continue;
                }
                let stop = self
                    .emphasis_pairs
                    .as_ref()
                    .unwrap()
                    .close(open + 2, marker);
                if let Some(close) = self.finish_close(bytes, marker, stop) {
                    ends.insert(open, (close, self.last_verbatim_start));
                }
            }
            self.last_verbatim_start = outer_verbatim;
            let strikes: Vec<_> = ends
                .iter()
                .filter(|(open, _)| bytes.get(**open + 1) == Some(&b'~'))
                .map(|(&open, &(close, _))| (open, close))
                .collect();
            let mut arrows = HashMap::new();
            if !strikes.is_empty() {
                let mut hosts = self.native_opaque.as_ref().unwrap_or(&self.opaque).clone();
                hosts.extend(&self.note_refs);
                for (&open, &(close, _)) in &ends {
                    hosts.insert(open, close + 2);
                }
                self.insert_editorial_hosts(bytes, &mut hosts);
                let mut scanner = SubstitutionScanner::default();
                for (open, close) in strikes {
                    if let Some(arrow) =
                        scanner.find(bytes, open + 2, close, &mut self.code_ends, &hosts)
                    {
                        arrows.insert(open, arrow);
                    }
                }
            }
            self.exported_native = Some(Rc::new(NativeBracedData {
                ends,
                arrows,
                attributes: self.attribute_tails.clone(),
                bare: self.bare_attribute_pairs.clone(),
                bare_at_start: self.bare_attribute_pairs_at_start.clone(),
            }));
        }
        NativeBracedView {
            data: Rc::clone(self.exported_native.as_ref().unwrap()),
            offset: start,
            length,
        }
    }

    /// Matched `{+ +}` and `{- -}` frames bound a substitution's arrow scan (PART 9 E2a).
    fn insert_editorial_hosts(&mut self, bytes: &[u8], hosts: &mut HashMap<usize, usize>) {
        let outer_verbatim = self.last_verbatim_start;
        for at in 0..bytes.len().saturating_sub(1) {
            if bytes[at] != b'{' || !matches!(bytes[at + 1], b'+' | b'-') {
                continue;
            }
            if let Some(close) = self.close_editorial(bytes, at, bytes[at + 1]) {
                hosts.entry(at).or_insert(close + 2);
            }
        }
        self.last_verbatim_start = outer_verbatim;
    }

    /// The end of the editorial comment opening at `at`. The closer scan steps
    /// over every closed `{#...#}` (`edScan`), glued to a host or not.
    fn comment_end(&mut self, bytes: &[u8], at: usize) -> Option<usize> {
        if bytes.get(at..at + 2) != Some(b"{#") || crate::parse::decorated_braced_opener(bytes, at)
        {
            return None;
        }
        let closes = self.comment_closes.get_or_insert_with(|| {
            (0..bytes.len().saturating_sub(1))
                .filter(|&at| bytes[at] == b'#' && bytes[at + 1] == b'}')
                .collect()
        });
        let close = *closes.get(closes.partition_point(|&close| close <= at + 2))?;
        Some(close + 2)
    }

    pub(super) fn with_hosts(opaque: HashMap<usize, usize>) -> Self {
        Self {
            opaque,
            ..Self::default()
        }
    }

    pub(super) fn set_note_refs(&mut self, note_refs: HashMap<usize, usize>) {
        self.note_refs = note_refs;
    }

    fn editorial_host(&self, at: usize) -> Option<usize> {
        self.note_refs
            .get(&at)
            .or_else(|| self.opaque.get(&at))
            .copied()
    }

    pub(super) fn substitution(&mut self, bytes: &[u8], open: usize) -> Option<(usize, usize)> {
        if bytes.get(open + 1) != Some(&b'~') {
            return None;
        }
        let view = self.code_view(bytes);
        let bytes = view.as_ref();
        let pair = self.close(bytes, open, b'~')?;
        if let Some(view) = &self.native_view {
            let arrow = *view.data.arrows.get(&(open + view.offset))?;
            let arrow = arrow.checked_sub(view.offset)?;
            return (arrow < pair).then(|| (self.new_half_close(bytes, arrow, pair), arrow));
        }
        if self.substitution_hosts.is_none() {
            let mut hosts = self.native_opaque.as_ref().unwrap_or(&self.opaque).clone();
            hosts.extend(&self.note_refs);
            let points = self.emphasis_pairs.as_ref().unwrap().points.clone();
            for at in points {
                if bytes.get(at) != Some(&b'{') {
                    continue;
                }
                let Some(&marker) = bytes.get(at + 1) else {
                    continue;
                };
                if !matches!(marker, b'/' | b'*' | b'_' | b'^' | b',' | b'~' | b'=') {
                    continue;
                }
                let stop = self.emphasis_pairs.as_ref().unwrap().close(at + 2, marker);
                if let Some(close) = stop.and_then(|stop| self.resolve_stop(bytes, marker, stop)) {
                    hosts.insert(at, close + 2);
                }
            }
            self.insert_editorial_hosts(bytes, &mut hosts);
            self.substitution_hosts = Some(hosts);
        }
        self.substitutions
            .find(
                bytes,
                open + 2,
                pair,
                &mut self.code_ends,
                self.substitution_hosts.as_ref().unwrap(),
            )
            .map(|arrow| (self.new_half_close(bytes, arrow, pair), arrow))
    }

    /// A reference the strike's table read as an image hides no `~}` from the
    /// new half (`edSubNew`), so the first `~}` past the arrow closes there.
    fn new_half_close(&mut self, bytes: &[u8], arrow: usize, pair: usize) -> usize {
        if !self.note_refs.keys().any(|&at| at > arrow && at < pair) {
            return pair;
        }
        match self.scan(bytes, arrow + 2, b'~', 0) {
            Ok(Some(close)) if close & RAW_CLOSE_FLAG == 0 && close < pair => close,
            _ => pair,
        }
    }

    pub(super) fn after_attributes(&mut self, bytes: &[u8], end: usize) -> usize {
        if let Some(view) = &self.native_view {
            return view
                .data
                .attributes
                .get(&(end + view.offset))
                .filter(|&&tail| tail <= view.offset + view.length)
                .map_or(end, |&tail| tail - view.offset);
        }
        if !*self.has_attribute_candidates.get_or_insert_with(|| {
            bytes.windows(2).any(|pair| {
                pair[0] == b'{'
                    && (pair[1].is_ascii_alphabetic() || matches!(pair[1], b'.' | b'#' | b':'))
            })
        }) {
            return end;
        }
        if self.emphasis_pairs.is_none() {
            self.build_emphasis_pairs(bytes);
        }
        self.attribute_tails.get(&end).copied().unwrap_or(end)
    }

    pub(super) fn bare_attributes(&mut self, bytes: &[u8], open: usize) -> Option<(usize, usize)> {
        self.bare_attributes_from(bytes, open, false)
    }

    /// As `bare_attributes`, for an opener whose guard `_` or `/` before it
    /// opens a span itself and so reads as START (the note under the templates).
    pub(super) fn bare_attributes_at_start(
        &mut self,
        bytes: &[u8],
        open: usize,
    ) -> Option<(usize, usize)> {
        self.bare_attributes_from(bytes, open, true)
    }

    fn bare_attributes_from(
        &mut self,
        bytes: &[u8],
        open: usize,
        at_start: bool,
    ) -> Option<(usize, usize)> {
        if let Some(view) = &self.native_view {
            let pairs = if at_start {
                &view.data.bare_at_start
            } else {
                &view.data.bare
            };
            let &(close, end) = pairs.get(&(open + view.offset))?;
            return (end <= view.offset + view.length)
                .then_some((close - view.offset, end - view.offset));
        }
        if !*self.has_attribute_candidates.get_or_insert_with(|| {
            bytes.windows(2).any(|pair| {
                pair[0] == b'{'
                    && (pair[1].is_ascii_alphabetic() || matches!(pair[1], b'.' | b'#' | b':'))
            })
        }) {
            return None;
        }
        if self.emphasis_pairs.is_none() {
            self.build_emphasis_pairs(bytes);
        }
        if at_start {
            self.bare_attribute_pairs_at_start.get(&open).copied()
        } else {
            self.bare_attribute_pairs.get(&open).copied()
        }
    }

    pub(super) fn close(&mut self, bytes: &[u8], open: usize, delim: u8) -> Option<usize> {
        if !matches!(delim, b'/' | b'*' | b'_' | b'^' | b',' | b'~' | b'=') {
            return self.close_editorial(bytes, open, delim);
        }
        if let Some(view) = &self.native_view {
            let pair = view.data.ends.get(&(open + view.offset)).copied();
            self.last_verbatim_start = pair
                .and_then(|(_, raw)| raw)
                .and_then(|at| at.checked_sub(view.offset))
                .filter(|&at| at < view.length);
            return pair
                .filter(|(close, _)| *close + 2 <= view.offset + view.length)
                .map(|(close, _)| close - view.offset);
        }
        self.queries += 1;
        if self.emphasis_pairs.is_none() {
            self.build_emphasis_pairs(bytes);
        }
        let result = self.emphasis_pairs.as_ref().unwrap().close(open + 2, delim);
        self.finish_close(bytes, delim, result)
    }

    fn code_view(&mut self, source: &[u8]) -> Rc<[u8]> {
        self.code_view
            .get_or_insert_with(|| mask_literal_directive_backticks(source).into())
            .clone()
    }

    fn close_editorial(&mut self, source: &[u8], open: usize, delim: u8) -> Option<usize> {
        self.queries += 1;
        let view = self.code_view(source);
        let bytes = view.as_ref();
        let result = if let Some(dense) = &self.dense {
            dense.close(open + 2, delim)
        } else {
            match self.scan(bytes, open + 2, delim, 0) {
                Ok(result) => result,
                Err(()) => {
                    self.pending = 0;
                    self.states = HashMap::new();
                    self.build_dense(bytes);
                    self.dense.as_ref().unwrap().close(open + 2, delim)
                }
            }
        };
        self.finish_close(bytes, delim, result)
    }

    fn finish_close(&mut self, bytes: &[u8], delim: u8, result: Option<usize>) -> Option<usize> {
        self.last_verbatim_start = result
            .filter(|stop| stop & RAW_CLOSE_FLAG != 0)
            .map(|stop| stop & !RAW_CLOSE_FLAG);
        result.and_then(|stop| self.resolve_stop(bytes, delim, stop))
    }

    pub(super) fn last_verbatim_start(&self) -> Option<usize> {
        self.last_verbatim_start
    }

    fn resolve_stop(&mut self, bytes: &[u8], delim: u8, stop: usize) -> Option<usize> {
        if stop & RAW_CLOSE_FLAG != 0 {
            self.raw_close(bytes, stop & !RAW_CLOSE_FLAG, delim)
        } else {
            Some(stop)
        }
    }

    fn code_end(&mut self, bytes: &[u8], start: usize) -> Option<usize> {
        if self.queries == 1 && self.pending == 0 {
            return skip_code_span(bytes, start);
        }
        self.code_ends
            .get_or_insert_with(|| CodeSpanIndex::new(bytes))
            .end(start)
    }

    fn raw_close(&mut self, bytes: &[u8], start: usize, delim: u8) -> Option<usize> {
        if self.queries == 1 && self.pending == 0 {
            return find_seq(bytes, start, &[delim, b'}']);
        }
        let closers = self.raw_closers.get_or_insert_with(|| {
            let mut closers: [Vec<usize>; DELIM_BRACE_SLOTS] = Default::default();
            for (at, pair) in bytes.windows(2).enumerate() {
                if pair[1] == b'}' {
                    if let Some(slot) = delim_brace_slot(pair[0]) {
                        closers[slot].push(at);
                    }
                }
            }
            Box::new(closers)
        });
        let positions = &closers[delim_brace_slot(delim)?];
        positions
            .get(positions.partition_point(|&at| at < start))
            .copied()
    }

    fn has_closer(&mut self, bytes: &[u8], from: usize, delim: u8) -> bool {
        let last = self.last_closers.get_or_insert_with(|| {
            let mut last = [None; DELIM_BRACE_SLOTS];
            for (at, pair) in bytes.windows(2).enumerate() {
                if pair[1] == b'}' {
                    if let Some(slot) = delim_brace_slot(pair[0]) {
                        last[slot] = Some(at);
                    }
                }
            }
            Box::new(last)
        });
        last[delim_brace_slot(delim).unwrap()].is_some_and(|at| at >= from)
    }

    /// Dense cap-sensitive inputs use rolling depth layers. Only scan events
    /// occupy table slots; two layers bound memory independently of the cap.
    fn build_dense(&mut self, bytes: &[u8]) {
        self.queries = self.queries.max(2);
        let mut points = Vec::new();
        let mut comments = HashMap::new();
        for at in 0..bytes.len().saturating_sub(1) {
            if self.editorial_host(at).is_none() {
                if let Some(end) = self.comment_end(bytes, at) {
                    comments.insert(at, end);
                }
            }
        }
        for at in 0..bytes.len().saturating_sub(1) {
            let next = bytes[at + 1];
            if (bytes[at] == b'`'
                && (at == 0 || bytes[at - 1] != b'`' || (at > 1 && bytes[at - 2] == b'\\')))
                || self.editorial_host(at).is_some()
                || comments.contains_key(&at)
                || bytes[at] == b'\\'
                || (bytes[at] == b'{'
                    && matches!(
                        next,
                        b'/' | b'*' | b'_' | b'^' | b',' | b'~' | b'=' | b'+' | b'-'
                    ))
                || (next == b'}' && delim_brace_slot(bytes[at]).is_some())
            {
                points.push(at);
            }
        }
        points.push(bytes.len());
        let index = |at| points.partition_point(|&point| point < at);
        let mut steps = Vec::new();
        let mut after_pair = Vec::new();
        for &at in &points[..points.len() - 1] {
            after_pair.push(index(at + 2));
            let step = if let Some(end) = self
                .editorial_host(at)
                .or_else(|| comments.get(&at).copied())
            {
                BracedStep::Jump(index(end))
            } else {
                match bytes[at] {
                    b'\\' => BracedStep::Jump(index((at + 2).min(bytes.len()))),
                    b'`' => match self.code_end(bytes, at) {
                        Some(end) => BracedStep::Jump(index(end)),
                        // An unclosed run takes the rest of the block (carve#3973 X5).
                        None => BracedStep::Raw(Box::new([usize::MAX; DELIM_BRACE_SLOTS])),
                    },
                    b'{' => {
                        BracedStep::Scope(delim_brace_slot(bytes[at + 1]).unwrap(), index(at + 2))
                    }
                    delim => BracedStep::Closer(delim_brace_slot(delim).unwrap()),
                }
            };
            steps.push(step);
        }
        let mut previous: [Vec<usize>; DELIM_BRACE_SLOTS] =
            std::array::from_fn(|_| vec![usize::MAX; points.len()]);
        let mut current = previous.clone();
        for depth in (0..=MAX_NESTING_DEPTH).rev() {
            for at in (0..steps.len()).rev() {
                for slot in 0..DELIM_BRACE_SLOTS {
                    current[slot][at] = match &steps[at] {
                        BracedStep::Jump(next) => current[slot][*next],
                        BracedStep::Raw(raw) => {
                            if raw[slot] == usize::MAX {
                                usize::MAX
                            } else {
                                at | RAW_CLOSE_FLAG
                            }
                        }
                        BracedStep::Closer(closer) if *closer == slot => at,
                        BracedStep::Scope(kind, from)
                            if *kind != slot && depth < MAX_NESTING_DEPTH =>
                        {
                            let encoded = previous[*kind][*from];
                            let close = if encoded == usize::MAX {
                                usize::MAX
                            } else if encoded & RAW_CLOSE_FLAG != 0 {
                                match &steps[encoded & !RAW_CLOSE_FLAG] {
                                    BracedStep::Raw(raw) => raw[*kind],
                                    _ => unreachable!(),
                                }
                            } else {
                                encoded
                            };
                            if close != usize::MAX && points[close] > points[at] + 2 {
                                current[slot][after_pair[close]]
                            } else {
                                current[slot][at + 1]
                            }
                        }
                        _ => current[slot][at + 1],
                    };
                }
            }
            let stable = current == previous;
            std::mem::swap(&mut previous, &mut current);
            if stable {
                break;
            }
        }
        self.dense = Some(Box::new(DenseBracedClosers {
            points,
            stops: previous,
        }));
    }

    fn extend_native_opaque(&mut self, bytes: &[u8]) -> Vec<(usize, usize)> {
        let last_brace = bytes.iter().rposition(|&byte| byte == b'}');
        let last_percent = bytes.windows(2).rposition(|pair| pair == b"%}");
        let last_editorial = bytes.windows(2).rposition(|pair| pair == b"#}");
        let mut next_comments = [0usize; 2];
        let mut attributes = Vec::new();
        let mut label_ends = Vec::new();
        let mut last_label_end = None;
        // PART 4: an attribute run is opaque only where it attaches to a host;
        // one after plain text is text (`x{title="*}"}`).
        let mut host_ends: std::collections::HashSet<usize> = self
            .opaque
            .iter()
            .filter(|&(&open, _)| match bytes[open] {
                b'[' | b'(' | b'`' => true,
                b'<' => bytes.get(open + 1) != Some(&b'/'),
                _ => false,
            })
            .map(|(_, &end)| end)
            .collect();
        let mut bare_openers = [false; 5];
        let mut braced_depth = [0usize; 5];
        // A `{X` with no `X}` after it is unmatched, so its `X` may open bare.
        let last_closers: [Option<usize>; 5] = std::array::from_fn(|slot| {
            let marker = b"/*_~="[slot];
            bytes.windows(2).rposition(|pair| pair == [marker, b'}'])
        });
        let mut at = 0;
        while at + 1 < bytes.len() {
            while label_ends.last().is_some_and(|&end| end <= at) {
                last_label_end = label_ends.pop();
            }
            if bytes[at] == b'\\' {
                at += 2;
                continue;
            }
            if let Some(slot) = bare_slot(bytes[at]) {
                let before = at.checked_sub(1).map(|prev| bytes[prev]);
                let braced =
                    before == Some(b'{') && last_closers[slot].is_some_and(|close| close > at);
                if braced {
                    braced_depth[slot] += 1;
                } else if bytes[at + 1] == b'}' && braced_depth[slot] > 0 {
                    braced_depth[slot] -= 1;
                }
                // E3: a bare opener inside a same-kind braced span is literal.
                bare_openers[slot] |= !braced
                    && braced_depth[slot] == 0
                    && !before
                        .is_some_and(|byte| byte.is_ascii_alphanumeric() || byte == bytes[at])
                    && !matches!(bytes[at + 1], b' ' | b'\t' | b'\r' | b'\n')
                    && bytes[at + 1] != bytes[at];
            }
            if let Some(&end) = self.opaque.get(&at) {
                if bytes[at] == b'[' {
                    label_ends.push(end - 1);
                } else {
                    at = end;
                    continue;
                }
            }
            let attribute_end = if bytes[at] == b'{'
                && (bytes[at + 1].is_ascii_alphabetic()
                    || matches!(bytes[at + 1], b'.' | b'#' | b':'))
                && attribute_host_before(bytes, at, &host_ends, &bare_openers)
            {
                read_attrs_at(bytes, at, last_brace).map(|(_, end)| end)
            } else {
                None
            };
            if let Some(end) = attribute_end {
                attributes.push((at, end));
                host_ends.insert(end);
            }
            let end = match bytes[at] {
                b'`' => self
                    .code_end(bytes, at)
                    .map(|end| raw_inline_format_end(bytes, end).map_or(end, |close| close + 1)),
                b'<' => scanned_autolink_end(bytes, at).map(|close| close + 1),
                b'%' if bytes[at + 1] == b'%'
                    && (at == 0
                        || matches!(bytes[at - 1], b' ' | b'\n')
                        || bytes[at - 1] == b'[' && self.opaque.contains_key(&(at - 1))) =>
                {
                    let end = trailing_comment_end(bytes, at + 2, label_ends.last().copied())
                        .unwrap_or(bytes.len());
                    self.line_comment_ends.insert(at, end);
                    Some(end)
                }
                b'{' if bytes[at + 1] == b'#' && last_label_end == at.checked_sub(1) => {
                    read_attrs_at(bytes, at, last_brace)
                        .map(|(_, end)| end)
                        .or_else(|| {
                            if !last_editorial.is_some_and(|close| close >= at + 2) {
                                return None;
                            }
                            if next_comments[1] < at + 2 {
                                next_comments[1] =
                                    find_seq(bytes, at + 2, b"#}").unwrap_or(bytes.len());
                            }
                            let close = next_comments[1];
                            (close < bytes.len() && close > at + 2).then_some(close + 2)
                        })
                }
                b'{' => {
                    let marker = bytes[at + 1];
                    let last = match marker {
                        b'%' => last_percent,
                        b'#' => last_editorial,
                        _ => None,
                    };
                    if last.is_some_and(|close| close >= at + 2) {
                        let slot = usize::from(marker == b'#');
                        if next_comments[slot] < at + 2 {
                            next_comments[slot] =
                                find_seq(bytes, at + 2, &[marker, b'}']).unwrap_or(bytes.len());
                        }
                        let close = next_comments[slot];
                        (close < bytes.len() && (marker == b'%' || close > at + 2))
                            .then_some(close + 2)
                    } else {
                        attribute_end
                    }
                }
                _ => None,
            };
            if let Some(end) = end {
                if bytes[at] == b'`' || bytes[at] == b'<' && bytes[at + 1] != b'/' {
                    host_ends.insert(end);
                }
                self.opaque.insert(at, end);
                at = attribute_end.unwrap_or(end);
            } else {
                at = attribute_end.unwrap_or(at + 1);
            }
        }
        attributes
    }

    fn build_emphasis_pairs(&mut self, bytes: &[u8]) {
        let view = self.code_view(bytes);
        let bytes = view.as_ref();
        #[cfg(test)]
        NATIVE_PAIR_SCAN_BYTES.with(|work| work.set(work.get() + bytes.len()));
        self.queries = self.queries.max(2);
        let original_opaque = self.opaque.clone();
        let mut attribute_tails = HashMap::new();
        for (start, end) in self.extend_native_opaque(bytes).into_iter().rev() {
            attribute_tails.insert(start, attribute_tails.get(&end).copied().unwrap_or(end));
        }
        let mut bare_attribute_pairs = HashMap::new();
        let mut bare_attribute_pairs_at_start = HashMap::new();
        let mut bare_slots = [false; DELIM_BRACE_SLOTS];
        for &start in attribute_tails.keys() {
            if let Some(&marker) = start.checked_sub(1).and_then(|at| bytes.get(at)) {
                if matches!(marker, b'/' | b'*' | b'_' | b'~' | b'=') {
                    bare_slots[delim_brace_slot(marker).unwrap()] = true;
                }
            }
        }
        // A block a bare closer takes hides a closer only through its opener,
        // which checks the guards in front of it; the guard kinds need tables too.
        let bare_hosted: std::collections::HashSet<usize> = attribute_tails
            .keys()
            .filter(|&&start| {
                start > 0 && matches!(bytes[start - 1], b'/' | b'*' | b'_' | b'~' | b'=')
            })
            .copied()
            .collect();
        if !bare_hosted.is_empty() {
            for marker in *b"/*_~=" {
                bare_slots[delim_brace_slot(marker).unwrap()] = true;
            }
        }
        let mut points = Vec::new();
        let bracket_ends: std::collections::HashSet<usize> = self
            .opaque
            .iter()
            .filter_map(|(&open, &end)| {
                (bytes.get(open) == Some(&b'[') && bytes.get(end.saturating_sub(1)) == Some(&b']'))
                    .then_some(end - 1)
            })
            .collect();
        let mut opener_marks = std::collections::HashSet::new();
        // `{X` markers met while a bare X is still open: as a guard such a
        // marker is a nested X, which does not open (E3).
        let mut nested_marks = std::collections::HashSet::new();
        let mut pending = [false; DELIM_BRACE_SLOTS];
        let mut at = 0;
        while at + 1 < bytes.len() {
            if bytes[at] == b'\\' {
                at += 2;
                continue;
            }
            // `{X}` is characters (CARVE-P3-014): its X is no opener mark.
            if bytes[at] == b'{'
                && delim_brace_slot(bytes[at + 1]).is_some()
                && bytes.get(at + 2) != Some(&b'}')
            {
                if pending[delim_brace_slot(bytes[at + 1]).unwrap()] {
                    nested_marks.insert(at + 1);
                }
                opener_marks.insert(at + 1);
                at += 2;
                continue;
            }
            if let Some(&end) = self.opaque.get(&at) {
                at = end.max(at + 1);
                continue;
            }
            if bytes[at] == b'`' {
                if let Some(end) = self.code_end(bytes, at) {
                    at = end.max(at + 1);
                    continue;
                }
            }
            let marker = bytes[at];
            if matches!(marker, b'/' | b'*' | b'_' | b'~' | b'=') {
                let slot = delim_brace_slot(marker).unwrap();
                let before = at.checked_sub(1).map(|prev| bytes[prev]);
                let next = bytes[at + 1];
                if pending[slot]
                    && before.is_some_and(|byte| !matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
                    && !next.is_ascii_alphanumeric()
                {
                    pending[slot] = false;
                } else if !matches!(next, b' ' | b'\t' | b'\r' | b'\n')
                    && next != marker
                    && before != Some(marker)
                    && !before.is_some_and(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
                    && (!matches!(marker, b'/' | b'_') || before != Some(b'/'))
                {
                    pending[slot] = true;
                }
            }
            at += 1;
        }
        for at in 0..bytes.len() {
            let next = bytes.get(at + 1).copied().unwrap_or(0);
            if (bytes[at] == b'`'
                && (at == 0 || bytes[at - 1] != b'`' || (at > 1 && bytes[at - 2] == b'\\')))
                || self.opaque.contains_key(&at)
                || bytes[at] == b'\\'
                || (bytes[at] == b'{'
                    && matches!(
                        next,
                        b'/' | b'*' | b'_' | b'^' | b',' | b'~' | b'=' | b'+' | b'-'
                    ))
                || bracket_ends.contains(&at)
                || delim_brace_slot(bytes[at]).is_some_and(|slot| bare_slots[slot])
                || (next == b'}' && delim_brace_slot(bytes[at]).is_some())
            {
                points.push(at);
            }
        }
        points.push(bytes.len());
        let index = |at| points.partition_point(|&point| point < at);
        let mut steps = Vec::new();
        for &at in &points[..points.len() - 1] {
            let step = if bracket_ends.contains(&at) {
                BracedStep::Boundary
            } else if let Some(&end) = self.line_comment_ends.get(&at) {
                let mut raw = [usize::MAX; DELIM_BRACE_SLOTS];
                for &delim in b"+-~#/*_^,=" {
                    let slot = delim_brace_slot(delim).unwrap();
                    raw[slot] = self
                        .raw_close(bytes, at, delim)
                        .filter(|&close| close < end)
                        .map_or(usize::MAX, index);
                }
                BracedStep::LineComment(Box::new(raw), index(end))
            } else if let Some(&end) = self.opaque.get(&at) {
                let end = if matches!(bytes[at], b'[' | b'<' | b'(' | b'`') {
                    attribute_tails.get(&end).copied().unwrap_or(end)
                } else {
                    end
                };
                BracedStep::Jump(index(end))
            } else {
                match bytes[at] {
                    b'\\' => BracedStep::Jump(index((at + 2).min(bytes.len()))),
                    b'`' => match self.code_end(bytes, at) {
                        Some(end) => {
                            let end =
                                raw_inline_format_end(bytes, end).map_or(end, |close| close + 1);
                            BracedStep::Jump(index(
                                attribute_tails.get(&end).copied().unwrap_or(end),
                            ))
                        }
                        // An unclosed run takes the rest of the block (carve#3973 X5).
                        None => BracedStep::Raw(Box::new([usize::MAX; DELIM_BRACE_SLOTS])),
                    },
                    b'{' => {
                        BracedStep::Scope(delim_brace_slot(bytes[at + 1]).unwrap(), index(at + 2))
                    }
                    _ if opener_marks.contains(&at) => BracedStep::Literal,
                    delim if bytes.get(at + 1) == Some(&b'}') => {
                        BracedStep::Closer(delim_brace_slot(delim).unwrap())
                    }
                    _ => BracedStep::Literal,
                }
            };
            steps.push(step);
        }
        // Per bare kind, the point of an opener still pending before each point
        // (none closed after it), outside skipped regions and after the last
        // braced opener: a span that is open there.
        let bare_kinds = *b"/*_~=";
        let mut pending_open: [Vec<usize>; 5] = std::array::from_fn(|_| Vec::new());
        if !bare_hosted.is_empty() {
            let mut state = [usize::MAX; 5];
            let mut skip_until = 0;
            for kind in &mut pending_open {
                kind.reserve(steps.len());
            }
            for (i, step) in steps.iter().enumerate() {
                for (k, kind) in pending_open.iter_mut().enumerate() {
                    kind.push(state[k]);
                }
                if i < skip_until {
                    continue;
                }
                match step {
                    BracedStep::Jump(next) | BracedStep::LineComment(_, next) => {
                        if !bare_hosted.contains(&points[i]) {
                            skip_until = *next;
                        }
                    }
                    BracedStep::Boundary | BracedStep::Scope(_, _) => state = [usize::MAX; 5],
                    BracedStep::Raw(_) => skip_until = usize::MAX,
                    BracedStep::Literal | BracedStep::Closer(_) => {
                        let position = points[i];
                        let marker = bytes[position];
                        if opener_marks.contains(&position) {
                            continue;
                        }
                        let Some(k) = bare_kinds.iter().position(|&d| d == marker) else {
                            continue;
                        };
                        let before = position.checked_sub(1).map(|at| bytes[at]);
                        let next = bytes.get(position + 1).copied();
                        if state[k] != usize::MAX
                            && before
                                .is_some_and(|byte| !matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
                            && !next.is_some_and(|byte| byte.is_ascii_alphanumeric())
                        {
                            state[k] = usize::MAX;
                        } else if state[k] == usize::MAX
                            && next.is_some_and(|byte| {
                                !matches!(byte, b' ' | b'\t' | b'\r' | b'\n') && byte != marker
                            })
                            && before != Some(marker)
                            && (marker != b'=' || next != Some(b'>'))
                            && (before.is_some_and(|_| opener_marks.contains(&(position - 1)))
                                || !before.is_some_and(|byte| {
                                    byte.is_ascii_alphanumeric()
                                        || byte == b'_'
                                        || byte == b'/' && matches!(marker, b'/' | b'_')
                                }))
                        {
                            state[k] = i;
                        }
                    }
                }
            }
        }
        let mut current: [Vec<usize>; DELIM_BRACE_SLOTS] =
            std::array::from_fn(|_| vec![usize::MAX; points.len()]);
        let mut bare: [Vec<usize>; DELIM_BRACE_SLOTS] = std::array::from_fn(|slot| {
            if bare_slots[slot] {
                vec![usize::MAX; points.len()]
            } else {
                Vec::new()
            }
        });
        for at in (0..steps.len()).rev() {
            let position = points[at];
            let marker = bytes[position];
            let before = position
                .checked_sub(1)
                .and_then(|at| bytes.get(at))
                .copied();
            let next = bytes.get(position + 1).copied();
            let unguarded_opener = next.is_some_and(|byte| {
                !matches!(byte, b' ' | b'\t' | b'\r' | b'\n') && byte != marker
            }) && before != Some(marker)
                && !before.is_some_and(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
                && (marker != b'=' || next != Some(b'>'))
                && (!matches!(marker, b'/' | b'_') || before != Some(b'/'));
            let resolve = |encoded: usize, slot: usize| {
                if encoded == usize::MAX {
                    usize::MAX
                } else if encoded & RAW_CLOSE_FLAG != 0 {
                    match &steps[encoded & !RAW_CLOSE_FLAG] {
                        BracedStep::Raw(raw) => raw[slot],
                        _ => unreachable!(),
                    }
                } else {
                    encoded
                }
            };
            let child_close = if let BracedStep::Scope(kind, from) = &steps[at] {
                let close = resolve(current[*kind][*from], *kind);
                // An empty group `{XX}` owns no closer (CARVE-P3-014).
                (close != usize::MAX && points[close] > position + 2).then_some(close)
            } else {
                None
            };
            let child_after = child_close.map(|close| {
                let end = points[close] + 2;
                index(attribute_tails.get(&end).copied().unwrap_or(end))
            });
            for slot in 0..DELIM_BRACE_SLOTS {
                if !bare_slots[slot] {
                    continue;
                }
                bare[slot][at] = match &steps[at] {
                    BracedStep::Boundary | BracedStep::Raw(_) => usize::MAX,
                    BracedStep::LineComment(_, next) => bare[slot][*next],
                    BracedStep::Jump(next) => bare[slot][*next],
                    BracedStep::Scope(_, _) if child_after.is_some() => {
                        bare[slot][child_after.unwrap()]
                    }
                    _ if delim_brace_slot(marker) == Some(slot)
                        && before
                            .is_some_and(|byte| !matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
                        && !next.is_some_and(|byte| byte.is_ascii_alphanumeric()) =>
                    {
                        at
                    }
                    _ => bare[slot][at + 1],
                };
            }
            let bare_slot = delim_brace_slot(marker).filter(|&slot| bare_slots[slot]);
            let bare_open = bare_slot.filter(|_| unguarded_opener);
            let bare_close = bare_open
                .map(|slot| bare[slot][at + 1])
                .filter(|&close| close != usize::MAX);
            let mut bare_tail = bare_close
                .and_then(|close| attribute_tails.get(&(points[close] + 1)))
                .copied();
            if let (Some(close), Some(end)) = (bare_close, bare_tail) {
                bare_attribute_pairs.insert(position, (points[close], end));
                // A span of another kind open here that closes inside the pair
                // demotes this opener (E2), so its block is content.
                let crossed = !pending_open[0].is_empty()
                    && bare_kinds.iter().enumerate().any(|(k, &d)| {
                        let slot = delim_brace_slot(d).unwrap();
                        d != marker
                            && pending_open[k][at] != usize::MAX
                            && bare[slot].get(at + 1).is_some_and(|&outer| {
                                outer != usize::MAX && points[outer] < points[close]
                            })
                    });
                if crossed {
                    bare_tail = None;
                }
            }
            let mut guarded = None;
            if matches!(before, Some(b'/' | b'_')) {
                let start_close = bare_slot
                    .filter(|_| {
                        next.is_some_and(|byte| {
                            !matches!(byte, b' ' | b'\t' | b'\r' | b'\n') && byte != marker
                        }) && (marker != b'=' || next != Some(b'>'))
                    })
                    .map(|slot| bare[slot][at + 1])
                    .filter(|&close| close != usize::MAX);
                if let Some(close) = start_close {
                    if let Some(&end) = attribute_tails.get(&(points[close] + 1)) {
                        bare_attribute_pairs_at_start.insert(position, (points[close], end));
                        if bare_open.is_none() && before != Some(marker) {
                            guarded = Some((bare_slot.unwrap(), close, end));
                        }
                    }
                }
            }
            // A guarded pair forms only where each guard in front of it is an open
            // entry where it stands (carve#3973 X1): a guard of the walk's own kind
            // is literal (E3).
            let guards_open = |slot: usize, pair: usize, end: usize, current: &[Vec<usize>]| {
                let span = resolve(current[slot][index(end)], slot);
                let from = index(end);
                let mut guard = position - 1;
                loop {
                    let kind = delim_brace_slot(bytes[guard]).unwrap();
                    let close = bare[kind][from];
                    // The walk's own `{X` guards as a bare X when it does not form.
                    // A first closer inside the pair would leave the pair's opener blocked.
                    if kind == slot && guard > 0 && bytes[guard - 1] == b'{' {
                        let first = bare[kind][at + 1];
                        return !nested_marks.contains(&guard)
                            && (first == usize::MAX || points[first] > points[pair])
                            && (close != usize::MAX || span != usize::MAX);
                    }
                    if kind == slot {
                        return false;
                    }
                    let outer = guard.checked_sub(1).map(|at| bytes[at]);
                    match outer {
                        Some(byte) if byte == bytes[guard] => return false,
                        Some(b'_') => {}
                        Some(b'/') if bytes[guard] == b'_' => {}
                        _ => return !outer.is_some_and(|byte| byte.is_ascii_alphanumeric()),
                    }
                    guard -= 1;
                }
            };
            for slot in 0..DELIM_BRACE_SLOTS {
                let mut stop = match &steps[at] {
                    BracedStep::Boundary => usize::MAX,
                    BracedStep::Jump(_) if bare_hosted.contains(&position) => current[slot][at + 1],
                    BracedStep::Jump(next) => current[slot][*next],
                    BracedStep::LineComment(raw, next) => {
                        if raw[slot] == usize::MAX {
                            current[slot][*next]
                        } else {
                            raw[slot]
                        }
                    }
                    BracedStep::Raw(raw) => {
                        if raw[slot] == usize::MAX {
                            usize::MAX
                        } else {
                            at | RAW_CLOSE_FLAG
                        }
                    }
                    BracedStep::Closer(closer) if *closer == slot => at,
                    BracedStep::Scope(kind, _)
                        if (*kind != slot || !matches!(*kind, 0 | 1)) && child_after.is_some() =>
                    {
                        current[slot][child_after.unwrap()]
                    }
                    _ => current[slot][at + 1],
                };
                if matches!(steps[at], BracedStep::Literal | BracedStep::Closer(_)) {
                    if let (Some(kind), Some(close), Some(end)) = (bare_open, bare_close, bare_tail)
                    {
                        if kind != slot
                            && (stop == usize::MAX
                                || points[stop & !RAW_CLOSE_FLAG] >= points[close])
                        {
                            stop = current[slot][index(end)];
                        }
                    } else if let Some((kind, close, end)) = guarded {
                        if kind != slot
                            && (stop == usize::MAX
                                || points[stop & !RAW_CLOSE_FLAG] >= points[close])
                            && guards_open(slot, close, end, &current)
                        {
                            stop = current[slot][index(end)];
                        }
                    }
                }
                current[slot][at] = stop;
            }
        }
        self.attribute_tails = attribute_tails;
        self.bare_attribute_pairs = bare_attribute_pairs;
        self.bare_attribute_pairs_at_start = bare_attribute_pairs_at_start;
        self.native_opaque = Some(std::mem::replace(&mut self.opaque, original_opaque));
        self.emphasis_pairs = Some(Box::new(DenseBracedClosers {
            points,
            stops: current,
        }));
    }

    fn state_depth(&mut self, bytes: &[u8], depth: usize, at: usize) -> usize {
        let thresholds = self.depth_thresholds.get_or_insert_with(|| {
            let openers: Vec<usize> = bytes
                .windows(2)
                .enumerate()
                .filter_map(|(at, pair)| {
                    (pair[0] == b'{'
                        && matches!(
                            pair[1],
                            b'/' | b'*' | b'_' | b'^' | b',' | b'~' | b'=' | b'+' | b'-'
                        ))
                    .then_some(at)
                })
                .collect();
            Box::new(std::array::from_fn(|depth| {
                let remaining = MAX_NESTING_DEPTH - depth;
                if remaining == 0 {
                    Some(usize::MAX)
                } else {
                    openers
                        .len()
                        .checked_sub(remaining)
                        .map(|index| openers[index])
                }
            }))
        });
        // With fewer remaining openers than available depth, the cap cannot
        // affect this suffix. Share its result across those depth contexts.
        if thresholds[depth].map_or(true, |last| at > last) {
            MAX_NESTING_DEPTH + 1
        } else {
            depth
        }
    }

    /// Where a scan for `delim}` starting at `from` stops.
    ///
    /// A backtick run's closer is searched for across the rest of the block, so a
    /// closer inside a closed code span is code (ruling markup-carve/carve#2079).
    /// A run with no closer ends at this pair's closer (markup-carve/carve#2056).
    /// An escaped character opens or closes no span outside verbatim content.
    fn scan(
        &mut self,
        bytes: &[u8],
        from: usize,
        delim: u8,
        depth: usize,
    ) -> Result<Option<usize>, ()> {
        let indexed = self.queries > 1 || depth > 0;
        let mut visited = Vec::new();
        let mut j = from;
        let mut resume = true;
        let result = loop {
            if j + 1 >= bytes.len() {
                break None;
            }
            if indexed && (resume || matches!(bytes[j], b'\\' | b'`' | b'{') || bytes[j] == delim) {
                let state_depth = self.state_depth(bytes, depth, j);
                if let Some(&result) = self.states.get(&(delim, state_depth, j)) {
                    break result;
                }
                if self.states.len() + self.pending >= bytes.len().max(1024) {
                    return Err(());
                }
                self.pending += 1;
                visited.push((j, state_depth));
            }
            resume = false;
            #[cfg(test)]
            {
                self.steps += 1;
            }
            if let Some(end) = self
                .editorial_host(j)
                .or_else(|| self.comment_end(bytes, j))
            {
                j = end;
                resume = true;
                continue;
            }
            match bytes[j] {
                b'\\' => {
                    j += 2;
                    resume = true;
                }
                b'`' => match self.code_end(bytes, j) {
                    Some(end) => {
                        j = end;
                        resume = true;
                    }
                    // An unclosed run takes the rest of the block, so the pair
                    // has no closer (carve#3973 X5).
                    None => break None,
                },
                b if b == delim && bytes[j + 1] == b'}' => break Some(j),
                b'{' if bytes[j + 1] != delim
                    && matches!(
                        bytes[j + 1],
                        b'/' | b'*' | b'_' | b'^' | b',' | b'~' | b'=' | b'+' | b'-'
                    )
                    && depth < MAX_NESTING_DEPTH =>
                {
                    match if self.has_closer(bytes, j + 2, bytes[j + 1]) {
                        self.scan(bytes, j + 2, bytes[j + 1], depth + 1)?
                            .and_then(|stop| self.resolve_stop(bytes, bytes[j + 1], stop))
                    } else {
                        None
                    } {
                        Some(close) if close > j + 2 => {
                            j = close + 2;
                            resume = true;
                        }
                        _ => j += 1,
                    }
                }
                _ => j += 1,
            }
        };
        self.pending -= visited.len();
        for (at, state_depth) in visited {
            self.states.insert((delim, state_depth, at), result);
        }
        Ok(result)
    }
}

enum BracedStep {
    Boundary,
    Literal,
    LineComment(Box<[usize; DELIM_BRACE_SLOTS]>, usize),
    Jump(usize),
    Raw(Box<[usize; DELIM_BRACE_SLOTS]>),
    Scope(usize, usize),
    Closer(usize),
}

struct DenseBracedClosers {
    points: Vec<usize>,
    stops: [Vec<usize>; DELIM_BRACE_SLOTS],
}

impl DenseBracedClosers {
    fn close(&self, from: usize, delim: u8) -> Option<usize> {
        let at = self.points.partition_point(|&point| point < from);
        let stop = *self.stops[delim_brace_slot(delim)?].get(at)?;
        (stop != usize::MAX).then(|| self.points[stop & !RAW_CLOSE_FLAG] | (stop & RAW_CLOSE_FLAG))
    }
}

/// Whether the brace group at `at` follows an inline element it can attach to:
/// a code span, autolink, bracket or destination, an earlier attribute block,
/// a braced closer, or a bare closer with an opener candidate before it.
fn attribute_host_before(
    bytes: &[u8],
    at: usize,
    host_ends: &std::collections::HashSet<usize>,
    bare_openers: &[bool; 5],
) -> bool {
    if host_ends.contains(&at) {
        return true;
    }
    if at < 2 {
        return false;
    }
    match bytes[at - 1] {
        b'}' => matches!(
            bytes[at - 2],
            b'/' | b'*' | b'_' | b'^' | b',' | b'~' | b'=' | b'+' | b'-'
        ),
        marker => bare_slot(marker).is_some_and(|slot| {
            bare_openers[slot]
                && !matches!(bytes[at - 2], b' ' | b'\t' | b'\r' | b'\n' | b'{' | b'\\')
        }),
    }
}

fn bare_slot(byte: u8) -> Option<usize> {
    match byte {
        b'/' => Some(0),
        b'*' => Some(1),
        b'_' => Some(2),
        b'~' => Some(3),
        b'=' => Some(4),
        _ => None,
    }
}

#[cfg(test)]
mod braced_closer_memo_tests {
    use super::*;

    #[test]
    fn nested_native_spans_share_the_payload_index() {
        let source = "{*".repeat(32) + &"a {#i#} ".repeat(1024) + &"*}".repeat(32);
        NATIVE_PAIR_SCAN_BYTES.with(|work| work.set(0));
        let document = crate::parse(&source);
        assert_eq!(
            crate::render_html(&document)
                .unwrap()
                .matches("<strong>")
                .count(),
            32
        );
        assert!(NATIVE_PAIR_SCAN_BYTES.with(|work| work.get()) < source.len() * 2);
    }

    #[test]
    fn nested_native_strikes_share_substitution_boundaries() {
        let source = "{~".repeat(32) + &"x~>y ".repeat(1024) + &"~}".repeat(32);
        NATIVE_PAIR_SCAN_BYTES.with(|work| work.set(0));
        let document = crate::parse(&source);
        let html = crate::render_html(&document).unwrap();
        assert_eq!(html.matches("<s>").count(), 31);
        assert!(html.contains("<ins>"));
        assert!(NATIVE_PAIR_SCAN_BYTES.with(|work| work.get()) < source.len() * 2);
    }
    fn reference_close(bytes: &[u8], open: usize, delim: u8, depth: usize) -> Option<usize> {
        let mut j = open + 2;
        while j + 1 < bytes.len() {
            match bytes[j] {
                b'\\' => j += 2,
                // An unclosed run takes the rest of the block (carve#3973 X5).
                b'`' => j = skip_code_span(bytes, j)?,
                b if b == delim && bytes[j + 1] == b'}' => return Some(j),
                b'{' if bytes[j + 1] != delim
                    && matches!(
                        bytes[j + 1],
                        b'/' | b'*' | b'_' | b'^' | b',' | b'~' | b'=' | b'+' | b'-'
                    )
                    && depth < MAX_NESTING_DEPTH =>
                {
                    match reference_close(bytes, j, bytes[j + 1], depth + 1) {
                        Some(close) if close > j + 2 => j = close + 2,
                        _ => j += 1,
                    }
                }
                _ => j += 1,
            }
        }
        None
    }

    #[test]
    fn dense_hosts_preserve_queries_inside_and_outside_labels() {
        let text = "{*a [{*b*}]{} [x](u*}) c*}";
        let label = text.find('[').unwrap();
        let label_end = text.find(']').unwrap() + 1;
        let destination = text.find('(').unwrap();
        let destination_end = text.find(')').unwrap() + 1;
        let hosts = HashMap::from([(label, label_end), (destination, destination_end)]);
        let mut sparse = BracedClosers::with_hosts(hosts.clone());
        let mut dense = BracedClosers::with_hosts(hosts);
        dense.build_dense(text.as_bytes());
        for (open, expected) in [
            (0, text.rfind("*}").unwrap()),
            (label + 1, text.find("*}").unwrap()),
        ] {
            assert_eq!(
                sparse.close_editorial(text.as_bytes(), open, b'*'),
                Some(expected)
            );
            assert_eq!(
                dense.close_editorial(text.as_bytes(), open, b'*'),
                Some(expected)
            );
        }
    }

    #[test]
    fn sparse_and_dense_keep_verbatim_closer_origins() {
        let text = "{*a`b [c*}d]";
        let hosts = HashMap::from([(6, 12)]);
        let mut sparse = BracedClosers::with_hosts(hosts.clone());
        let mut dense = BracedClosers::with_hosts(hosts);
        dense.build_dense(text.as_bytes());
        for memo in [&mut sparse, &mut dense] {
            // The unclosed run takes the rest of the block (carve#3973 X5).
            assert_eq!(memo.close_editorial(text.as_bytes(), 0, b'*'), None);
            assert_eq!(memo.last_verbatim_start(), None);
        }
    }

    #[test]
    fn repeated_label_closers_share_forward_states() {
        let text = "{,a [b,}] ".repeat(4000) + ",}";
        let mut hosts = HashMap::new();
        for (at, byte) in text.bytes().enumerate() {
            if byte == b'[' {
                hosts.insert(at, at + 5);
            }
        }
        let mut memo = BracedClosers::with_hosts(hosts);
        for open in (0..text.len() - 2).step_by(10) {
            assert_eq!(
                memo.close_editorial(text.as_bytes(), open, b','),
                Some(text.len() - 2)
            );
        }
        assert!(memo.steps < text.len() * 8, "{} steps", memo.steps);
    }

    #[test]
    fn cached_states_match_the_scanner_across_opaque_and_nested_content() {
        let tokens = [
            "{_", "{*", "{~", "{+", "{-", "{=", "_}", "*}", "~}", "+}", "-}", "=}", "`", "``",
            "\\`", "[", "]", "x", "}",
        ];
        let mut seed = 17_u64;
        for case in 0..600 {
            let mut text = String::new();
            for _ in 0..30 {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                text.push_str(tokens[(seed >> 32) as usize % tokens.len()]);
            }
            let bytes = text.as_bytes();
            let mut memo = BracedClosers::default();
            let mut dense = BracedClosers::default();
            if case < 50 {
                dense.build_dense(bytes);
            }
            for open in 0..bytes.len().saturating_sub(1) {
                if bytes[open] != b'{' {
                    continue;
                }
                for delim in *b"_*~+-=" {
                    assert_eq!(
                        memo.close_editorial(bytes, open, delim),
                        reference_close(bytes, open, delim, 0),
                        "production query: {text:?}, {open}, {delim}"
                    );
                    if case < 50 {
                        assert_eq!(
                            dense.close_editorial(bytes, open, delim),
                            reference_close(bytes, open, delim, 0),
                            "dense query: {text:?}, {open}, {delim}"
                        );
                    }
                    for depth in [0, MAX_NESTING_DEPTH - 1, MAX_NESTING_DEPTH] {
                        let mut capped = BracedClosers {
                            queries: 2,
                            ..Default::default()
                        };
                        assert_eq!(
                            capped
                                .scan(bytes, open + 2, delim, depth)
                                .unwrap()
                                .and_then(|stop| capped.resolve_stop(bytes, delim, stop)),
                            reference_close(bytes, open, delim, depth),
                            "{text:?}, {open}, {delim}, {depth}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn the_first_alternating_scope_scan_shares_plain_suffix_states() {
        let text = "{_".to_string() + &"{*{_".repeat(40) + &"x".repeat(100_000) + "_}";
        let mut memo = BracedClosers::default();
        assert!(memo.close_editorial(text.as_bytes(), 0, b'_').is_some());
        assert!(memo.steps < text.len() * 8, "{} steps", memo.steps);
        assert!(
            memo.states.len() < text.len() * 4,
            "{} states",
            memo.states.len()
        );
    }

    #[test]
    fn dense_layers_match_the_reference_scan() {
        for text in [
            "{_{*{_x_}",
            "{*`*}`",
            "{_\\```_} ``",
            "{+{~x~>y~}+}",
            "{_[_}]",
            "{_{*x*}_}",
        ] {
            let mut memo = BracedClosers::default();
            memo.build_dense(text.as_bytes());
            for open in 0..text.len().saturating_sub(1) {
                if text.as_bytes()[open] != b'{' {
                    continue;
                }
                for delim in *b"_*~+-=" {
                    assert_eq!(
                        memo.close_editorial(text.as_bytes(), open, delim),
                        reference_close(text.as_bytes(), open, delim, 0),
                        "{text}, {open}"
                    );
                }
            }
        }
    }

    #[test]
    fn cap_sensitive_dense_openers_keep_a_bounded_cache() {
        let text = "{/".to_string() + &"{*{_{^{,{~{={+{-".repeat(512) + "*}_}^},}~}=}+}-}/}";
        let mut memo = BracedClosers::default();
        memo.close_editorial(text.as_bytes(), 0, b'/');
        assert!(memo.states.len() <= text.len() * 4);
        let dense = memo
            .dense
            .as_ref()
            .expect("cap-sensitive scans use rolling layers");
        assert!(dense.points.len() <= text.len());
    }

    #[test]
    fn repeated_openers_visit_a_bounded_number_of_scan_states() {
        for n in [1024, 4096] {
            for suffix in ["`_}`", "[_}]", "{*`_}`*}"] {
                let text = "{_k=x}".repeat(n) + suffix;
                let mut memo = BracedClosers::default();
                for open in (0..n * 6).step_by(6) {
                    memo.close_editorial(text.as_bytes(), open, b'_');
                }
                assert!(
                    memo.steps <= text.len() * 4,
                    "{n}, {suffix}, {} steps",
                    memo.steps
                );
            }
        }
    }

    #[test]
    fn the_first_nested_escaped_tick_scan_builds_one_run_index() {
        let text = "{_".to_string() + &"{+\\```+}".repeat(1024) + "_}";
        let mut memo = BracedClosers::default();
        assert_eq!(
            memo.close_editorial(text.as_bytes(), 0, b'_'),
            reference_close(text.as_bytes(), 0, b'_', 0)
        );
        assert_eq!(memo.code_ends.as_ref().unwrap().runs.len(), 1024);
        // An unclosed run has no closer to search for (carve#3973 X5).
        assert!(memo.raw_closers.is_none());
    }

    #[test]
    fn rolling_layers_keep_the_depth_cap_semantics() {
        for n in [199, 201, 205] {
            let text = "{*{_".repeat(n) + "x" + &"_}*}".repeat(n);
            let mut memo = BracedClosers::default();
            memo.build_dense(text.as_bytes());
            assert_eq!(
                memo.close_editorial(text.as_bytes(), 0, b'*'),
                reference_close(text.as_bytes(), 0, b'*', 0)
            );
        }
    }

    #[test]
    fn rolling_layers_do_not_allocate_slots_for_each_backtick_suffix() {
        let text = "{*".to_string() + &"`".repeat(100_000) + "*}";
        let mut memo = BracedClosers::default();
        memo.build_dense(text.as_bytes());
        assert!(memo.dense.as_ref().unwrap().points.len() < 8);
        assert_eq!(
            memo.close_editorial(text.as_bytes(), 0, b'*'),
            reference_close(text.as_bytes(), 0, b'*', 0)
        );
    }
}
