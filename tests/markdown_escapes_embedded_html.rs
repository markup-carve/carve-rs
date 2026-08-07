//! The Markdown target neutralizes embedded HTML everywhere it writes author
//! content.
//!
//! The writer states that invariant next to `escape_md_html`: carve's "HTML is
//! text" guarantee holds for this target too, so Markdown re-rendered to HTML
//! cannot execute. Math content, the abbreviation definition line and the
//! footnote label skipped it (`markup-carve/carve-rs#807`).

fn markdown(source: &str) -> String {
    let doc = carve::parse(source);
    carve::render_markdown(&doc).expect("markdown renders")
}

const PAYLOAD: &str = "<script>alert(1)</script>";
const ESCAPED: &str = "&lt;script&gt;alert(1)&lt;/script&gt;";

#[test]
fn math_content_is_escaped() {
    let out = markdown("a $`<script>alert(1)</script>` b\n");
    assert!(!out.contains(PAYLOAD), "raw HTML survived in math: {out}");
    assert!(out.contains(ESCAPED), "{out}");
}

#[test]
fn display_math_content_is_escaped() {
    let out = markdown("$$`<script>alert(1)</script>`\n");
    assert!(!out.contains(PAYLOAD), "raw HTML survived in math: {out}");
}

/// The occurrence's `<abbr title=...>` was already escaped, and the definition
/// line one arm away was not - one output disagreeing with itself.
#[test]
fn the_abbreviation_definition_line_is_escaped() {
    let out = markdown("*[AB]: <script>alert(1)</script>\n\nAB\n");
    assert!(
        !out.contains(PAYLOAD),
        "raw HTML survived in the definition line: {out}"
    );
    assert_eq!(
        out.matches(ESCAPED).count(),
        2,
        "the definition and the occurrence should escape alike: {out}"
    );
}

/// The abbreviation KEY escapes too. The parser will not accept a `<` in a
/// term, so this slot is only reachable through AST ingest - which is a caller
/// handing over a tree from a database row or a bridge, exactly the input that
/// has no parser in front of it.
#[test]
fn an_ingested_abbreviation_key_is_escaped() {
    let doc = carve::parse("*[AB]: exp\n\nAB\n");
    let json = carve::to_json(&doc).replace("\"AB\"", "\"<script>\"");
    let ingested = carve::from_json(&json).expect("the patched tree is still valid");
    let out = carve::render_markdown(&ingested).expect("markdown renders");

    assert!(
        !out.contains("<script>"),
        "raw HTML survived in a key: {out}"
    );
    assert_eq!(
        out.matches("&lt;script&gt;").count(),
        2,
        "the definition line and the occurrence should escape alike: {out}"
    );
}

/// Both positions escape, so the reference still matches its definition in the
/// emitted Markdown.
#[test]
fn a_footnote_label_is_escaped_in_both_positions() {
    let out = markdown("x[^<script>alert(1)</script>]\n\n[^<script>alert(1)</script>]: body\n");
    assert!(
        !out.contains(PAYLOAD),
        "raw HTML survived in a label: {out}"
    );
    assert!(out.contains(&format!("[^{ESCAPED}]")), "{out}");
    assert!(out.contains(&format!("[^{ESCAPED}]: ")), "{out}");
}

/// The UNRESOLVED reference is the same slot one branch over. It escaped its
/// brackets, because they are Markdown metacharacters, and skipped the HTML -
/// the escape decision was being made for one and not the other.
#[test]
fn an_unresolved_footnote_label_is_escaped() {
    let out = markdown("x[^<script>alert(1)</script>]\n");
    assert!(!out.contains(PAYLOAD), "raw HTML survived: {out}");
    assert!(
        out.contains("\\[^"),
        "the bracket escape is still there: {out}"
    );
}

/// An unresolved crossref keeps its authored marker (escaping `</#nope>` whole
/// would turn something a reader can act on into noise), but the TARGET inside
/// it is author content and can hold a `<`: `</#a<script>` is a complete
/// opening tag once the Markdown is rendered.
#[test]
fn an_unresolved_crossref_target_is_escaped() {
    let out = markdown("</#a<script>alert(1)</script>b>\n");
    assert!(
        !out.contains("<script>"),
        "the target closed an opening tag: {out}"
    );
    assert!(out.contains("</#a&lt;script>"), "{out}");
}

/// CONTROL: an ordinary unresolved crossref is left exactly as authored.
#[test]
fn an_ordinary_unresolved_crossref_is_untouched() {
    assert!(markdown("text </#nope> more\n").contains("</#nope>"));
}

/// The escape is transparent, not lossy: a consumer decodes the entity back to
/// the character before its math renderer sees the content, which is exactly
/// what the HTML target has always relied on.
#[test]
fn escaping_math_preserves_an_ordinary_comparison() {
    let out = markdown("$`a < b`\n");
    assert!(out.contains("a &lt; b"), "{out}");
}

/// CONTROL: content with no HTML in it is untouched on all three paths.
#[test]
fn ordinary_content_is_unchanged() {
    let out = markdown("*[HT]: Hypertext\n\nHT and $`x^2`$ and y[^n]\n\n[^n]: note\n");
    assert!(out.contains("*[HT]: Hypertext"), "{out}");
    assert!(out.contains("<abbr title=\"Hypertext\">HT</abbr>"), "{out}");
    assert!(out.contains("[^n]"), "{out}");
    assert!(out.contains("[^n]: note"), "{out}");
    assert!(
        !out.contains("&amp;"),
        "nothing here should be escaped: {out}"
    );
}
