fn strip(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            while let Some(&n) = chars.peek() {
                chars.next();
                if n == 'm' {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[test]
fn ansi_table_allots_two_columns_per_cjk_char() {
    let out = strip(&carve::to_ansi("| 日本 | b |\n|---|---|\n| 語 | y |\n"));
    assert!(out.contains("┌──────┬───┐"), "{out}");
    assert!(out.contains("│ 日本 │ b │"), "{out}");
    assert!(out.contains("│ 語   │ y │"), "{out}");
}
