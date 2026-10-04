use super::detect_comment_fence_line_any_column;
use std::collections::HashMap;
use std::ops::Range;

pub(super) struct ScopedFenceClosers {
    leaves: usize,
    code_runs: Vec<[usize; 2]>,
    comments: HashMap<usize, Vec<usize>>,
}

impl ScopedFenceClosers {
    pub(super) fn new<'a>(lines: impl ExactSizeIterator<Item = &'a str>) -> Self {
        let leaves = lines.len().max(1).next_power_of_two();
        let mut code_runs = vec![[0; 2]; leaves * 2];
        let mut comments: HashMap<usize, Vec<usize>> = HashMap::new();
        for (index, line) in lines.enumerate() {
            let bytes = line.trim_start().as_bytes();
            if let Some(&character @ (b'`' | b'~')) = bytes.first() {
                let run = bytes.iter().take_while(|&&byte| byte == character).count();
                if bytes[run..]
                    .iter()
                    .all(|&byte| byte == b' ' || byte == b'\t')
                {
                    code_runs[leaves + index][usize::from(character == b'~')] = run;
                }
            }
            if let Some(open) = detect_comment_fence_line_any_column(line) {
                comments.entry(open.fence_len).or_default().push(index);
            }
        }
        for index in (1..leaves).rev() {
            let left = code_runs[index * 2];
            let right = code_runs[index * 2 + 1];
            code_runs[index] = [left[0].max(right[0]), left[1].max(right[1])];
        }
        Self {
            leaves,
            code_runs,
            comments,
        }
    }

    pub(super) fn code_in(&self, range: Range<usize>, character: u8, width: usize) -> bool {
        if range.start >= range.end || !matches!(character, b'`' | b'~') {
            return false;
        }
        let character = usize::from(character == b'~');
        if self.code_runs[1][character] < width {
            return false;
        }
        let mut left = self.leaves + range.start;
        let mut right = self.leaves + range.end;
        while left < right {
            if left % 2 == 1 {
                if self.code_runs[left][character] >= width {
                    return true;
                }
                left += 1;
            }
            if right % 2 == 1 {
                right -= 1;
                if self.code_runs[right][character] >= width {
                    return true;
                }
            }
            left /= 2;
            right /= 2;
        }
        false
    }

    pub(super) fn comment_in(&self, range: Range<usize>, width: usize) -> bool {
        let Some(positions) = self.comments.get(&width) else {
            return false;
        };
        let end = positions.partition_point(|&position| position < range.end);
        end > 0 && positions[end - 1] >= range.start
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::{
        build_code_closer_last_index, build_comment_closer_last_index, code_closer_exists_after,
    };

    #[test]
    fn bounded_queries_match_fresh_slice_indices() {
        let lines = [
            "payload",
            "```",
            "~~~~\t",
            "``` text",
            "  ````` ",
            "\u{a0}~~~",
            "%%%",
            " %%%% tail",
            "  %%%",
            "`",
            "",
        ];
        let index = ScopedFenceClosers::new(lines.iter().copied());
        for start in 0..lines.len() {
            for end in start..=lines.len() {
                let code = build_code_closer_last_index(&lines[start..end]);
                let comments = build_comment_closer_last_index(&lines[start..end]);
                for at in start..end {
                    for width in 1..=8 {
                        for character in *b"`~" {
                            assert_eq!(
                                index.code_in(at + 1..end, character, width),
                                code_closer_exists_after(&code, at - start, character, width)
                            );
                        }
                        assert_eq!(
                            index.comment_in(at..end, width),
                            comments.get(&width).is_some_and(|&last| last >= at - start)
                        );
                    }
                }
            }
        }
    }
}
