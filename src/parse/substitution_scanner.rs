use super::braced_closers::CodeSpanIndex;
use super::{find_seq, skip_code_span};
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct SubstitutionScanner {
    first: Option<(usize, usize, Option<usize>)>,
    last_arrow: Option<Option<usize>>,
    index: Option<Box<ArrowIndex>>,
}

impl SubstitutionScanner {
    pub(super) fn find(
        &mut self,
        bytes: &[u8],
        from: usize,
        to: usize,
        code_ends: &mut Option<CodeSpanIndex>,
        hosts: &HashMap<usize, usize>,
    ) -> Option<usize> {
        let last = *self
            .last_arrow
            .get_or_insert_with(|| bytes.windows(2).rposition(|pair| pair == b"~>"));
        if last.map_or(true, |arrow| arrow < from) {
            return None;
        }
        match self.first {
            None => {
                let result = scan(bytes, from, to, hosts);
                self.first = Some((from, to, result));
                result
            }
            Some((old_from, old_to, result)) if old_from == from && old_to == to => result,
            _ => self
                .index
                .get_or_insert_with(|| Box::new(ArrowIndex::new(bytes, code_ends, hosts)))
                .find(bytes, from, to),
        }
    }
}

fn scan(bytes: &[u8], from: usize, to: usize, hosts: &HashMap<usize, usize>) -> Option<usize> {
    let last_percent = bytes.windows(2).rposition(|pair| pair == b"%}");
    let last_editorial = bytes.windows(2).rposition(|pair| pair == b"#}");
    let mut disabled = 0;
    let mut at = from;
    while at + 1 < to {
        if let Some(&end) = hosts.get(&at) {
            at = end;
            continue;
        }
        match bytes[at] {
            b'\\' => at += 2,
            b'`' => at = skip_code_span(bytes, at)?,
            b'{' if matches!(bytes[at + 1], b'%' | b'#') => {
                let kind = if bytes[at + 1] == b'%' { 1 } else { 2 };
                if disabled & kind != 0 {
                    at += 1;
                    continue;
                }
                let last = if bytes[at + 1] == b'%' {
                    last_percent
                } else {
                    last_editorial
                };
                let close = last
                    .filter(|&last| last >= at + 2)
                    .and_then(|_| find_seq(bytes, at + 2, &[bytes[at + 1], b'}']));
                if close.is_some_and(|close| close >= to) {
                    disabled |= kind;
                }
                at = close
                    .filter(|&close| close < to)
                    .map_or(at + 1, |close| close + 2);
            }
            b'~' if bytes[at + 1] == b'>' => return Some(at),
            _ => at += 1,
        }
    }
    None
}

struct ArrowIndex {
    points: Vec<usize>,
    paths: [ForwardPaths; 4],
    comments: Vec<Option<usize>>,
    kinds: Vec<usize>,
    next_events: Vec<usize>,
}

impl ArrowIndex {
    fn new(
        bytes: &[u8],
        code_ends: &mut Option<CodeSpanIndex>,
        hosts: &HashMap<usize, usize>,
    ) -> Self {
        let n = bytes.len();
        if bytes.contains(&b'`') {
            code_ends.get_or_insert_with(|| CodeSpanIndex::new(bytes));
        }
        let mut points: Vec<usize> = (0..n)
            .filter(|&at| {
                (bytes[at] == b'~' && bytes.get(at + 1) == Some(&b'>'))
                    || (bytes[at] == b'{'
                        && bytes
                            .get(at + 1)
                            .is_some_and(|next| matches!(next, b'%' | b'#')))
            })
            .collect();
        points.push(n);
        let mut next_events = vec![points.len() - 1; n + 2];
        let mut event = points.len() - 1;
        for at in (0..n).rev() {
            next_events[at] = if event > 0 && points[event - 1] == at {
                event -= 1;
                event
            } else if let Some(&end) = hosts.get(&at) {
                next_events[end]
            } else if bytes[at] == b'\\' {
                next_events[at + 2]
            } else if bytes[at] == b'`' {
                let end = code_ends.as_ref().unwrap().end(at);
                end.map_or(points.len() - 1, |end| next_events[end])
            } else {
                next_events[at + 1]
            };
        }
        let mut comments = vec![None; points.len()];
        let mut kinds = vec![0; points.len()];
        let mut percent = None;
        let mut editorial = None;
        let mut next_percent = None;
        let mut next_editorial = None;
        event = points.len() - 1;
        for at in (0..n).rev() {
            if bytes.get(at..at + 2) == Some(b"%}") {
                next_percent = percent;
                percent = Some(at);
            }
            if bytes.get(at..at + 2) == Some(b"#}") {
                next_editorial = editorial;
                editorial = Some(at);
            }
            if event > 0 && points[event - 1] == at {
                event -= 1;
                if bytes[at] == b'{' {
                    let is_percent = bytes[at + 1] == b'%';
                    let (close, next) = if is_percent {
                        (percent, next_percent)
                    } else {
                        (editorial, next_editorial)
                    };
                    comments[event] = close.filter(|&close| close >= at + 2).or(next);
                    kinds[event] = if is_percent { 1 } else { 2 };
                }
            }
        }
        let paths = std::array::from_fn(|mask| {
            let parents = points
                .iter()
                .enumerate()
                .map(|(i, &at)| {
                    if at == n || bytes[at] == b'~' {
                        i
                    } else if mask & kinds[i] == 0 && comments[i].is_some() {
                        next_events[comments[i].unwrap() + 2]
                    } else {
                        next_events[at + 1]
                    }
                })
                .collect();
            ForwardPaths::new(parents)
        });
        Self {
            points,
            paths,
            comments,
            kinds,
            next_events,
        }
    }

    fn find(&self, bytes: &[u8], from: usize, to: usize) -> Option<usize> {
        let mut at = self.next_events[from];
        let mut mask = 0;
        while self.points[at] + 1 < to {
            at = self.paths[mask].last_before(&self.points, at, to - 1);
            let pos = self.points[at];
            if bytes[pos] == b'~' {
                return Some(pos);
            }
            if mask & self.kinds[at] == 0 && self.comments[at].is_some_and(|close| close >= to) {
                mask |= self.kinds[at];
                at = self.next_events[pos + 1];
            } else {
                return None;
            }
        }
        None
    }
}

/// Heavy paths answer forward range queries with linear storage.
struct ForwardPaths {
    parents: Vec<usize>,
    heads: Vec<usize>,
    starts: Vec<usize>,
    lengths: Vec<usize>,
    order: Vec<usize>,
}

impl ForwardPaths {
    fn new(parents: Vec<usize>) -> Self {
        let n = parents.len();
        let mut sizes = vec![1; n];
        let mut heavy = vec![None; n];
        for at in 0..n {
            let parent = parents[at];
            if parent == at {
                continue;
            }
            sizes[parent] += sizes[at];
            if heavy[parent].map_or(true, |child| sizes[child] < sizes[at]) {
                heavy[parent] = Some(at);
            }
        }
        let mut heads = vec![0; n];
        let mut lengths = vec![0; n];
        for at in (0..n).rev() {
            let parent = parents[at];
            let head = if parent != at && heavy[parent] == Some(at) {
                heads[parent]
            } else {
                at
            };
            heads[at] = head;
            lengths[head] += 1;
        }
        let mut starts = vec![0; n];
        let mut cursor = 0;
        for at in 0..n {
            starts[at] = cursor;
            cursor += lengths[at];
        }
        let mut cursors = starts.clone();
        let mut order = vec![0; n];
        for (at, &head) in heads.iter().enumerate() {
            order[cursors[head]] = at;
            cursors[head] += 1;
        }
        Self {
            parents,
            heads,
            starts,
            lengths,
            order,
        }
    }

    fn last_before(&self, points: &[usize], mut at: usize, to: usize) -> usize {
        loop {
            let head = self.heads[at];
            if points[head] >= to {
                let mut low = self.starts[head];
                let mut high = low + self.lengths[head];
                while low < high {
                    let mid = (low + high) / 2;
                    if points[self.order[mid]] < to {
                        low = mid + 1;
                    } else {
                        high = mid;
                    }
                }
                return self.order[low - 1];
            }
            let parent = self.parents[head];
            if parent == head || points[parent] >= to {
                return head;
            }
            at = parent;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexed_queries_skip_label_and_destination_arrows() {
        let text = "{~a [x~>y](u~>z) b~>c~} {~d~>e~}";
        let hosts = HashMap::from([
            (text.find('[').unwrap(), text.find(']').unwrap() + 1),
            (text.find('(').unwrap(), text.find(')').unwrap() + 1),
        ]);
        let first = text.find(" b~>").unwrap() + 2;
        let second = text.rfind("~>").unwrap();
        let mut scanner = SubstitutionScanner::default();
        let mut code = None;
        for (from, to, expected) in [
            (0, text.len(), Some(first)),
            (1, text.len(), Some(first)),
            (first + 2, text.len(), Some(second)),
            (1, first, None),
        ] {
            assert_eq!(
                scanner.find(text.as_bytes(), from, to, &mut code, &hosts),
                expected
            );
        }
    }

    #[test]
    fn indexed_queries_keep_the_original_limit_and_opaque_rules() {
        let tokens = [
            "x", "~>", "{%", "{#", "%}", "#}", "`", "``", "\\", "{~", "~}",
        ];
        let mut seed = 19_u64;
        let mut sources = vec![
            "{#}~>x#}".to_owned(),
            "{%}~>x%}".to_owned(),
            "{%{#~>x#}%}~>".to_owned(),
        ];
        for _ in 0..120 {
            let mut source = String::new();
            for _ in 0..15 {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                source.push_str(tokens[(seed >> 32) as usize % tokens.len()]);
            }
            sources.push(source);
        }
        for source in sources {
            let bytes = source.as_bytes();
            let mut scanner = SubstitutionScanner::default();
            let mut code_ends = None;
            for from in 0..bytes.len() {
                for to in from..=bytes.len() {
                    assert_eq!(
                        scanner.find(bytes, from, to, &mut code_ends, &HashMap::new()),
                        scan(bytes, from, to, &HashMap::new()),
                        "{source}: {from}..{to}"
                    );
                }
            }
        }
    }
}
