use carve::lint::{LintOptions, lint};

fn count(source: &str, rule: &str) -> usize {
    lint(source, &LintOptions::default())
        .into_iter()
        .filter(|finding| finding.channel == "lint" && finding.code == rule)
        .count()
}

#[test]
fn lint_parity_with_carve_js_and_carve_php() {
    for (source, rule, expected) in [
        ("- [ ] a\n", "blank-reference-label", 0), // C2-B-046: a task box is no label
        ("a [ ]\n", "blank-reference-label", 0), // C2-I-072: a bracket pair with nothing glued to it is no reference
        ("x [a][ ]\n", "blank-reference-label", 1), // C2-I-079: a blank full reference label
        ("[ ]: u\n", "blank-reference-label", 1), // C2-I-079: a blank definition label
        ("![](i.png)\n", "blank-reference-label", 0), // C2-I-079: an image with no alt text holds no label
        (
            "::: box\n   :: t\n: d\n:::\n",
            "block-opener-without-blank-line",
            0,
        ), // C2-B-069: a term's later line is no opener candidate
        ("2) :: t\n    : {#id}\n", "markdown-definition-paragraph", 0), // C2-B-069: a term's later line is no paragraph line
        (
            ":::{.c}\n:: key\n:  key\n",
            "markdown-definition-paragraph",
            0,
        ), // C2-B-074: a colon line below a `::` line
        (": x\n", "markdown-definition-paragraph", 0), // C2-B-074: the first line of a paragraph
        ("a\n: x\n", "markdown-definition-paragraph", 1), // C2-B-074: a later paragraph line
        (
            ">>> |= A |= B |\n> | x | y |\n",
            "block-opener-without-blank-line",
            1,
        ), // C2-B-031: a quote run that is text leaves the next quote line a candidate
        (">*\n> : more\n", "markdown-definition-paragraph", 0), // C2-B-031: a glued mark is no container prefix
        ("a\n## b\n", "block-opener-without-blank-line", 1), // C2-B-151: a two-mark heading is a candidate
        ("a\n====== b\n", "block-opener-without-blank-line", 1), // C2-B-151: a six-mark heading is a candidate
        ("a\n####### b\n", "block-opener-without-blank-line", 0), // C2-B-151: seven marks are text
        ("[^n]: ^ x\n", "unattached-caption", 1), // C2-B-149: a caption line in a footnote body
        ("- [a]: u\n", "definition-in-container", 1), // C2-B-081: a definition on a list marker line
        (":: t\n: [a]: u\n", "definition-in-container", 1), // C2-B-081: a definition behind a description mark
        ("[^n]: ^ x\n", "definition-in-container", 0), // C2-B-081: a footnote definition is not reported for its own mark
        (":: > x\n", "definition-term-block-folded", 1), // C2-B-075: a quote mark opening a term's text
        (":: c\n# H\n", "definition-term-block-folded", 0), // C2-B-075: a block opener on a later line of the term
    ] {
        assert_eq!(count(source, rule), expected, "{rule} on {source:?}");
    }
}
