use crate::table_unicode_18_data::{GCB, INCB_CONSONANT, INCB_EXTEND, INCB_LINKER, PICT, WIDE};

fn property(ranges: &[u32], code: u32, stride: usize) -> u32 {
    let mut low = 0;
    let mut high = ranges.len() / stride;
    while low < high {
        let mid = (low + high) / 2;
        let offset = mid * stride;
        if code < ranges[offset] {
            high = mid;
        } else if code > ranges[offset + 1] {
            low = mid + 1;
        } else {
            return if stride == 3 { ranges[offset + 2] } else { 1 };
        }
    }
    0
}

const CR: u32 = 1;
const LF: u32 = 2;
const CONTROL: u32 = 3;
const EXTEND: u32 = 4;
const ZWJ: u32 = 5;
const RI: u32 = 6;
const PREPEND: u32 = 7;
const SPACING_MARK: u32 = 8;
const L: u32 = 9;
const V: u32 = 10;
const T: u32 = 11;
const LV: u32 = 12;
const LVT: u32 = 13;

#[derive(Clone, Copy)]
struct Character {
    start: usize,
    gcb: u32,
    pict: bool,
    consonant: bool,
    linker: bool,
    indic_extend: bool,
    wide: bool,
}

#[derive(Clone, Copy)]
struct Context {
    ri_run: usize,
    linker_before: bool,
    pict_before: bool,
}

fn characters(text: &str) -> Vec<Character> {
    text.char_indices()
        .map(|(start, ch)| {
            let code = ch as u32;
            Character {
                start,
                gcb: property(GCB, code, 3),
                pict: property(PICT, code, 2) != 0,
                consonant: property(INCB_CONSONANT, code, 2) != 0,
                linker: property(INCB_LINKER, code, 2) != 0,
                indic_extend: property(INCB_EXTEND, code, 2) != 0,
                wide: property(WIDE, code, 2) != 0,
            }
        })
        .collect()
}

fn contexts(chars: &[Character]) -> Vec<Context> {
    let mut out = Vec::with_capacity(chars.len());
    let mut ri_run = 0;
    let mut linker_before = false;
    let mut pict_before = false;
    for ch in chars {
        ri_run = if ch.gcb == RI { ri_run + 1 } else { 0 };
        if !(ch.indic_extend && !ch.linker) {
            linker_before = ch.linker;
        }
        if ch.gcb != EXTEND {
            pict_before = ch.pict;
        }
        out.push(Context {
            ri_run,
            linker_before,
            pict_before,
        });
    }
    out
}

fn break_before(chars: &[Character], context: &[Context], index: usize) -> bool {
    let left = chars[index - 1];
    let right = chars[index];
    let a = left.gcb;
    let b = right.gcb;
    if a == CR && b == LF {
        return false;
    }
    if [CR, LF, CONTROL].contains(&a) || [CR, LF, CONTROL].contains(&b) {
        return true;
    }
    if a == L && [L, V, LV, LVT].contains(&b) {
        return false;
    }
    if [LV, V].contains(&a) && [V, T].contains(&b) {
        return false;
    }
    if [LVT, T].contains(&a) && b == T {
        return false;
    }
    if [EXTEND, ZWJ, SPACING_MARK].contains(&b) || a == PREPEND {
        return false;
    }
    if right.consonant && context[index - 1].linker_before {
        return false;
    }
    if right.pict && a == ZWJ && index >= 2 && context[index - 2].pict_before {
        return false;
    }
    if a == RI && b == RI && context[index - 1].ri_run % 2 == 1 {
        return false;
    }
    true
}

fn cluster_ranges(
    text: &str,
    chars: &[Character],
    context: &[Context],
) -> Vec<(usize, usize, bool)> {
    let mut out = Vec::new();
    if chars.is_empty() {
        return out;
    }
    let mut start = 0;
    let mut wide = false;
    for (index, ch) in chars.iter().enumerate() {
        if index > 0 && break_before(chars, context, index) {
            out.push((start, ch.start, wide));
            start = ch.start;
            wide = false;
        }
        wide |= ch.wide;
    }
    out.push((start, text.len(), wide));
    out
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn table_grapheme_clusters(text: &str) -> Vec<&str> {
    let chars = characters(text);
    let context = contexts(&chars);
    cluster_ranges(text, &chars, &context)
        .into_iter()
        .map(|(start, end, _)| &text[start..end])
        .collect()
}

pub(crate) fn table_display_width(text: &str) -> usize {
    let chars = characters(text);
    let context = contexts(&chars);
    cluster_ranges(text, &chars, &context)
        .into_iter()
        .map(|(_, _, wide)| if wide { 2 } else { 1 })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_18_grapheme_break_cases() {
        let cases = include_str!("../tests/fixtures/grapheme-break-18.txt");
        let mut count = 0;
        for line in cases.lines() {
            let tokens = line
                .split('#')
                .next()
                .unwrap_or("")
                .split_whitespace()
                .collect::<Vec<_>>();
            if tokens.is_empty() {
                continue;
            }
            let mut source = String::new();
            let mut expected = Vec::<String>::new();
            let mut cluster = String::new();
            for pair in tokens.chunks(2) {
                if pair.len() != 2 {
                    break;
                }
                if pair[0] == "÷" && !cluster.is_empty() {
                    expected.push(std::mem::take(&mut cluster));
                }
                if let Ok(code) = u32::from_str_radix(pair[1], 16) {
                    let ch = char::from_u32(code).unwrap();
                    source.push(ch);
                    cluster.push(ch);
                }
            }
            if !cluster.is_empty() {
                expected.push(cluster);
            }
            assert!(!source.is_empty(), "{line}");
            let actual = table_grapheme_clusters(&source);
            assert_eq!(actual, expected, "{line}");
            count += 1;
        }
        assert!(count > 0);
    }

    #[test]
    fn width_examples_and_regional_indicators() {
        assert_eq!(table_display_width("山田太郎"), 8);
        assert_eq!(table_display_width("e\u{301}"), 1);
        assert_eq!(table_display_width("💻"), 2);
        assert_eq!(table_display_width("·"), 1);
        assert_eq!(table_display_width(""), 0);
        let run = "🇦".repeat(2001);
        assert_eq!(table_display_width(&run), 1001);
    }
}
