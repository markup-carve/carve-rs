/// Drop C0/C1 control characters (keeping tab and newline) from author content
/// so attacker `ESC` / OSC sequences cannot inject into terminal output (the
/// ANSI and plain-text renderers). The renderers' own styling escapes are added
/// separately and are not affected.
pub(crate) fn strip_controls(input: &str) -> String {
    input
        .chars()
        .filter(|c| *c == '\t' || *c == '\n' || !c.is_control())
        .collect()
}
