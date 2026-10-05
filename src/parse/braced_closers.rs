use super::substitution_scanner::SubstitutionScanner;
use super::{delim_brace_slot, find_seq, skip_code_span, DELIM_BRACE_SLOTS, MAX_NESTING_DEPTH};
use std::collections::HashMap;

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
    queries: usize,
    states: HashMap<(u8, usize, usize), Option<usize>>,
    substitutions: SubstitutionScanner,
    code_ends: Option<CodeSpanIndex>,
    raw_closers: Option<Box<[Vec<usize>; DELIM_BRACE_SLOTS]>>,
    depth_thresholds: Option<Box<[Option<usize>; MAX_NESTING_DEPTH + 1]>>,
    last_closers: Option<Box<[Option<usize>; DELIM_BRACE_SLOTS]>>,
    pending: usize,
    dense: Option<Box<DenseBracedClosers>>,
    #[cfg(test)]
    steps: usize,
}

impl BracedClosers {
    pub(super) fn substitution(&mut self, bytes: &[u8], open: usize) -> Option<(usize, usize)> {
        if bytes.get(open + 1) != Some(&b'~') {
            return None;
        }
        let pair = self.close(bytes, open, b'~')?;
        self.substitutions
            .find(bytes, open + 2, pair, &mut self.code_ends)
            .map(|arrow| (pair, arrow))
    }

    pub(super) fn close(&mut self, bytes: &[u8], open: usize, delim: u8) -> Option<usize> {
        self.queries += 1;
        if let Some(dense) = &self.dense {
            return dense.close(open + 2, delim);
        }
        match self.scan(bytes, open + 2, delim, 0) {
            Ok(result) => result,
            Err(()) => {
                self.pending = 0;
                self.states = HashMap::new();
                self.build_dense(bytes);
                self.dense.as_ref().unwrap().close(open + 2, delim)
            }
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
        for at in 0..bytes.len().saturating_sub(1) {
            let next = bytes[at + 1];
            if (bytes[at] == b'`'
                && (at == 0 || bytes[at - 1] != b'`' || (at > 1 && bytes[at - 2] == b'\\')))
                || (bytes[at] == b'\\' && next == b'`')
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
            let step = match bytes[at] {
                b'\\' if bytes[at + 1] == b'`' => BracedStep::Jump(index(at + 2)),
                b'`' => match self.code_end(bytes, at) {
                    Some(end) => BracedStep::Jump(index(end)),
                    None => {
                        let mut raw = [usize::MAX; DELIM_BRACE_SLOTS];
                        for &delim in b"+-~#/*_^,=" {
                            let slot = delim_brace_slot(delim).unwrap();
                            raw[slot] = self.raw_close(bytes, at, delim).map_or(usize::MAX, &index);
                        }
                        BracedStep::Raw(Box::new(raw))
                    }
                },
                b'{' => BracedStep::Scope(delim_brace_slot(bytes[at + 1]).unwrap(), index(at + 2)),
                delim => BracedStep::Closer(delim_brace_slot(delim).unwrap()),
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
                        BracedStep::Raw(raw) => raw[slot],
                        BracedStep::Closer(closer) if *closer == slot => at,
                        BracedStep::Scope(kind, from)
                            if *kind != slot && depth < MAX_NESTING_DEPTH =>
                        {
                            let close = previous[*kind][*from];
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
    /// An escaped backtick opens no span; other escapes are left alone.
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
            match bytes[j] {
                b'\\' if bytes[j + 1] == b'`' => {
                    j += 2;
                    resume = true;
                }
                b'`' => match self.code_end(bytes, j) {
                    Some(end) => {
                        j = end;
                        resume = true;
                    }
                    None => break self.raw_close(bytes, j, delim),
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
        (stop != usize::MAX).then(|| self.points[stop])
    }
}

#[cfg(test)]
mod braced_closer_memo_tests {
    use super::*;
    fn reference_close(bytes: &[u8], open: usize, delim: u8, depth: usize) -> Option<usize> {
        let mut j = open + 2;
        while j + 1 < bytes.len() {
            match bytes[j] {
                b'\\' if bytes[j + 1] == b'`' => j += 2,
                b'`' => match skip_code_span(bytes, j) {
                    Some(end) => j = end,
                    None => return find_seq(bytes, j, &[delim, b'}']),
                },
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
                        memo.close(bytes, open, delim),
                        reference_close(bytes, open, delim, 0),
                        "production query: {text:?}, {open}, {delim}"
                    );
                    if case < 50 {
                        assert_eq!(
                            dense.close(bytes, open, delim),
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
                            capped.scan(bytes, open + 2, delim, depth).unwrap(),
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
        assert!(memo.close(text.as_bytes(), 0, b'_').is_some());
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
                        memo.close(text.as_bytes(), open, delim),
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
        memo.close(text.as_bytes(), 0, b'/');
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
                    memo.close(text.as_bytes(), open, b'_');
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
            memo.close(text.as_bytes(), 0, b'_'),
            reference_close(text.as_bytes(), 0, b'_', 0)
        );
        assert_eq!(memo.code_ends.as_ref().unwrap().runs.len(), 1024);
        assert!(memo.raw_closers.is_some());
    }

    #[test]
    fn rolling_layers_keep_the_depth_cap_semantics() {
        for n in [199, 201, 205] {
            let text = "{*{_".repeat(n) + "x" + &"_}*}".repeat(n);
            let mut memo = BracedClosers::default();
            memo.build_dense(text.as_bytes());
            assert_eq!(
                memo.close(text.as_bytes(), 0, b'*'),
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
            memo.close(text.as_bytes(), 0, b'*'),
            reference_close(text.as_bytes(), 0, b'*', 0)
        );
    }
}
