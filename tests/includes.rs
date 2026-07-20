//! Processor-level file inclusion, spec PART 9 §19 (I1-I11).
//!
//! Vectors are ported from carve-js `test/includes.test.ts` so the two engines
//! are pinned to the same observable behavior; the second module below keys
//! coverage to the normative rules one by one.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use carve::{
    expand_includes, parse, render_html, FileSystemResolver, IncludeContext, IncludeDependency,
    IncludeOptions, IncludeResolved, IncludeResolver,
};

struct MapResolver {
    files: HashMap<String, String>,
    /// Records every path handed to the resolver, so a shielded directive can
    /// be proven never to reach it (I3/I9).
    calls: std::cell::RefCell<Vec<String>>,
    /// When set, strips a leading "./" to produce a canonical id.
    canonical_ids: bool,
}

impl MapResolver {
    fn new(files: &[(&str, &str)]) -> Self {
        Self {
            files: files
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect(),
            calls: std::cell::RefCell::new(Vec::new()),
            canonical_ids: false,
        }
    }

    fn with_canonical_ids(mut self) -> Self {
        self.canonical_ids = true;
        self
    }
}

impl IncludeResolver for MapResolver {
    fn resolve(&self, path: &str, _ctx: &IncludeContext<'_>) -> Option<IncludeResolved> {
        self.calls.borrow_mut().push(path.to_string());
        if self.canonical_ids {
            let id = path.strip_prefix("./").unwrap_or(path);
            return self
                .files
                .get(id)
                .map(|source| IncludeResolved::with_id(source.clone(), id));
        }
        self.files
            .get(path)
            .map(|s| IncludeResolved::from(s.clone()))
    }
}

struct Expanded {
    warnings: Vec<(String, Option<String>)>,
    messages: Vec<String>,
    dependencies: Vec<IncludeDependency>,
    html: String,
}

impl Expanded {
    fn rules(&self) -> Vec<&str> {
        self.warnings.iter().map(|(r, _)| r.as_str()).collect()
    }
}

fn expand_with(source: &str, resolver: &dyn IncludeResolver, opts: IncludeOptions<'_>) -> Expanded {
    let doc = parse(source);
    let result = expand_includes(doc, source, &opts.with_resolver(resolver));
    Expanded {
        warnings: result
            .warnings
            .iter()
            .map(|w| (w.rule.clone(), w.file.clone()))
            .collect(),
        messages: result.warnings.iter().map(|w| w.message.clone()).collect(),
        dependencies: result.dependencies,
        html: render_html(&result.doc),
    }
}

fn expand(source: &str, files: &[(&str, &str)]) -> Expanded {
    let resolver = MapResolver::new(files);
    expand_with(source, &resolver, IncludeOptions::new())
}

fn expand_opts(source: &str, files: &[(&str, &str)], opts: IncludeOptions<'_>) -> Expanded {
    let resolver = MapResolver::new(files);
    expand_with(source, &resolver, opts)
}

/// Byte-exact "literal" baseline: what the core renders with no resolver at all.
fn literal_html(source: &str) -> String {
    render_html(&parse(source))
}

fn dep(id: &str, resolved: bool) -> IncludeDependency {
    IncludeDependency {
        id: id.to_string(),
        resolved,
    }
}

// ---------------------------------------------------------------------------
// Ported from carve-js test/includes.test.ts
// ---------------------------------------------------------------------------

#[test]
fn no_resolver_leaves_include_directives_literal_without_warnings() {
    let source = "See {{ child.crv }} here.";
    let doc = parse(source);
    let result = expand_includes(doc, source, &IncludeOptions::new());
    assert!(result.warnings.is_empty());
    assert!(result.dependencies.is_empty());
    assert_eq!(render_html(&result.doc), "<p>See {{ child.crv }} here.</p>");
}

#[test]
fn verbatim_shielding_keeps_directives_literal_in_fences_and_code_spans() {
    let source = "```txt\n{{ child }}\n```\n\nUse `{{ child }}`.";
    let result = expand(source, &[("child", "expanded")]);
    assert!(result.warnings.is_empty());
    assert!(result.html.contains("{{ child }}"));
    assert!(!result.html.contains("expanded"));
}

#[test]
fn fragment_containment_keeps_an_unclosed_child_fence_from_swallowing_parent() {
    let result = expand(
        "Before.\n\n{{ child }}\n\nAfter.",
        &[("child", "```js\nlet x = 1;")],
    );
    assert!(result.warnings.is_empty());
    assert!(result
        .html
        .contains("<pre><code class=\"language-js\">let x = 1;\n</code></pre>"));
    assert!(result.html.contains("<p>After.</p>"));
}

#[test]
fn inline_include_of_multi_block_child_warns_and_stays_literal() {
    let result = expand("See {{ child }}.", &[("child", "One.\n\nTwo.")]);
    assert_eq!(result.rules(), vec!["include-block-in-inline"]);
    assert_eq!(result.html, "<p>See {{ child }}.</p>");
}

#[test]
fn cycle_depth_and_budget_limits_warn_and_leave_the_directive_literal() {
    let cycle = expand("{{ a }}", &[("a", "{{ b }}"), ("b", "{{ a }}")]);
    assert!(cycle.rules().contains(&"include-cycle"));
    assert!(cycle.html.contains("{{ a }}"));

    let depth = expand_opts(
        "{{ a }}",
        &[("a", "{{ b }}"), ("b", "done")],
        IncludeOptions::new().with_max_depth(1),
    );
    assert_eq!(depth.rules(), vec!["include-depth"]);
    assert!(depth.html.contains("{{ b }}"));

    let budget = expand_opts(
        "{{ a }}",
        &[("a", "too large")],
        IncludeOptions::new().with_max_bytes(1),
    );
    assert_eq!(budget.rules(), vec!["include-budget"]);
    assert_eq!(budget.html, "<p>{{ a }}</p>");
}

#[test]
fn section_includes_the_selected_heading_subtree() {
    let result = expand(
        "{{ child #pick }}",
        &[(
            "child",
            "# A\n\nskip\n\n{#pick}\n# B\n\nyes\n\n## C\n\nmore\n\n# D\n\nskip",
        )],
    );
    assert!(result.warnings.is_empty());
    assert!(result.html.contains("<section id=\"pick\">"));
    assert!(result.html.contains("<h1>B</h1>"));
    assert!(result.html.contains("<p>yes</p>"));
    assert!(result.html.contains("<h2>C</h2>"));
    assert!(result.html.contains("<p>more</p>"));
    assert!(!result.html.contains("skip"));
}

#[test]
fn lines_includes_an_inclusive_physical_line_range() {
    let result = expand(
        "{{ child @lines:2-3 }}",
        &[("child", "skip\nOne\nTwo\nskip")],
    );
    assert!(result.warnings.is_empty());
    assert_eq!(result.html, "<p>One\nTwo</p>");
}

#[test]
fn shift_shifts_headings_and_warns_when_clamped() {
    let result = expand("{{ child @shift:1 }}", &[("child", "# A\n\n###### B")]);
    assert_eq!(result.rules(), vec!["include-heading-clamp"]);
    assert!(result.html.contains("<h2>A</h2>"));
    assert!(result.html.contains("<h6>B</h6>"));
}

/// I11: `resolved` reports ONLY whether the target's source was read.
///
/// The file was read here - it simply had no such section - so the dependency
/// stays RESOLVED. A host that dropped it would stop watching the child, and
/// then adding the missing section to that child would never invalidate the
/// preview: the document would stay broken until an unrelated edit happened to
/// retrigger a build.
#[test]
fn a_missing_section_keeps_the_dependency_resolved_because_the_file_was_read() {
    let result = expand("{{ child #nope }}", &[("child", "# Real")]);
    assert_eq!(result.rules(), vec!["include-section"]);
    assert_eq!(result.dependencies, vec![dep("child", true)]);
}

/// The dividing line is strictly "did a read happen". The depth limit refuses
/// the target BEFORE handing it to the resolver, so nothing was read and it is
/// correctly unresolved - this pins the contrast against the test above so a
/// future sweep cannot flatten the two cases into one rule.
#[test]
fn a_depth_refused_target_stays_unresolved_because_no_read_happened() {
    let result = expand_opts(
        "{{ a }}",
        &[("a", "{{ b }}"), ("b", "deep")],
        IncludeOptions::new().with_max_depth(1),
    );
    assert_eq!(result.rules(), vec!["include-depth"]);
    assert_eq!(
        result.dependencies,
        vec![dep("a", true), dep("b", false)],
        "the root child was read; the depth-refused grandchild never was"
    );
}

#[test]
fn section_plus_lines_warns_and_stays_literal() {
    let source = "{{ child #x @lines:1-1 }}";
    let result = expand(source, &[("child", "# X")]);
    assert_eq!(result.rules(), vec!["include-selection-conflict"]);
    // Literal means byte-identical to the no-resolver render, tag/mention
    // markup for #x and @lines included.
    assert_eq!(result.html, literal_html(source));
    assert!(!result.html.contains("<h1>"));
}

#[test]
fn renames_duplicate_child_footnote_labels_keeping_each_reference_with_its_definition() {
    let result = expand(
        "{{ a }}\n\n{{ b }}",
        &[
            ("a", "First[^a].\n\n[^a]: one"),
            ("b", "Second[^a].\n\n[^a]: two"),
        ],
    );
    assert_eq!(result.rules(), vec!["include-footnote-rename"]);
    assert!(result.html.contains("First<a id=\"fnref1\""));
    assert!(result.html.contains("Second<a id=\"fnref2\""));
    assert!(result.html.contains("one"));
    assert!(result.html.contains("two"));
}

#[test]
fn an_inline_include_keeps_its_renamed_footnote_reference_with_its_own_definition() {
    // The spliced reference must follow the rename. Lifting the paragraph's
    // inlines out before the merge would rename the definition and leave the
    // reference pointing at the label the parent kept.
    let result = expand(
        "Parent[^a]. {{ child }}\n\n[^a]: parent body",
        &[("child", "Child[^a].\n\n[^a]: child body")],
    );
    assert_eq!(result.rules(), vec!["include-footnote-rename"]);
    assert!(result.html.contains("parent body"));
    assert!(result.html.contains("child body"));
    // Two distinct footnotes, so two distinct reference anchors.
    assert!(result.html.contains("fnref1"), "html: {}", result.html);
    assert!(result.html.contains("fnref2"), "html: {}", result.html);
}

#[test]
fn renames_duplicate_explicit_heading_ids_deterministically() {
    let result = expand(
        "{{ a }}\n\n{{ b }}",
        &[("a", "{#dup}\n# A"), ("b", "{#dup}\n# B")],
    );
    assert_eq!(result.rules(), vec!["include-heading-id-rename"]);
    assert!(result.html.contains("<section id=\"dup\">"));
    assert!(result.html.contains("<section id=\"dup-2\">"));
}

#[test]
fn parent_explicit_ids_win_a_collision_and_the_child_crossref_follows_the_rename() {
    let result = expand(
        "{{ a }}\n\n{#dup}\n# Parent",
        &[("a", "{#dup}\n# Child\n\nSee </#dup>.")],
    );
    assert_eq!(result.rules(), vec!["include-heading-id-rename"]);
    assert!(result.html.contains("<section id=\"dup-2\">"));
    assert!(result.html.contains("href=\"#dup-2\""));
}

#[test]
fn detects_a_cycle_through_differing_path_spellings_when_the_resolver_supplies_ids() {
    let resolver = MapResolver::new(&[("a", "{{ ./b }}"), ("b", "{{ a }}")]).with_canonical_ids();
    let result = expand_with("{{ a }}", &resolver, IncludeOptions::new());
    assert!(result.rules().contains(&"include-cycle"));
}

#[test]
fn expands_a_directive_mid_sentence_and_keeps_the_surrounding_text() {
    let result = expand(
        "Intro: {{ child }} tail.",
        &[("child", "a /short/ fragment")],
    );
    assert!(result.warnings.is_empty());
    assert_eq!(result.html, "<p>Intro: a <em>short</em> fragment tail.</p>");
}

#[test]
fn recognizes_options_the_core_split_into_tag_and_mention_nodes() {
    let result = expand(
        "{{ child #pick @shift:1 }}",
        &[("child", "{#pick}\n# B\n\nyes")],
    );
    assert!(result.warnings.is_empty());
    assert!(result.html.contains("<h2>B</h2>"));
}

#[test]
fn warns_on_an_unknown_option_and_leaves_the_directive_literal() {
    let source = "{{ child @nope:1 }}";
    let result = expand(source, &[("child", "text")]);
    assert_eq!(result.rules(), vec!["include-unknown-option"]);
    assert_eq!(result.html, literal_html(source));
}

#[test]
fn resolves_a_quoted_path_after_the_core_rewrites_it_to_typographic_quotes() {
    let result = expand(
        "{{ \"my chapter.crv\" }}",
        &[("my chapter.crv", "spaced path body")],
    );
    assert!(result.warnings.is_empty());
    assert_eq!(result.html, "<p>spaced path body</p>");
}

#[test]
fn reports_a_nested_include_chain_as_deduplicated_dependencies() {
    let result = expand(
        "{{ child }}",
        &[
            ("child", "Child.\n\n{{ grandchild }}"),
            ("grandchild", "Grandchild."),
        ],
    );
    assert!(result.warnings.is_empty());
    assert_eq!(
        result.dependencies,
        vec![dep("child", true), dep("grandchild", true)]
    );
}

#[test]
fn reports_a_missing_include_target_as_an_unresolved_dependency() {
    let result = expand("{{ present }}\n\n{{ absent }}", &[("present", "Here.")]);
    assert_eq!(result.rules(), vec!["include-unresolved"]);
    assert_eq!(
        result.dependencies,
        vec![dep("present", true), dep("absent", false)]
    );
}

#[test]
fn reports_the_same_file_included_twice_only_once() {
    let result = expand("{{ child }}\n\n{{ child }}", &[("child", "Body.")]);
    assert!(result.warnings.is_empty());
    assert_eq!(result.dependencies, vec![dep("child", true)]);
}

#[test]
fn reports_no_dependencies_without_a_resolver() {
    let source = "{{ child }}";
    let result = expand_includes(parse(source), source, &IncludeOptions::new());
    assert!(result.dependencies.is_empty());
}

// ---------------------------------------------------------------------------
// FileSystemResolver / containment (I10)
// ---------------------------------------------------------------------------

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let mut base = std::env::temp_dir();
        let unique = format!(
            "carve-includes-{tag}-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        );
        base.push(unique);
        fs::create_dir_all(&base).expect("temp dir");
        Self(base)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write(&self, rel: &str, contents: &str) -> PathBuf {
        let full = self.0.join(rel);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).expect("parent dir");
        }
        fs::write(&full, contents).expect("write");
        full
    }

    fn mkdir(&self, rel: &str) -> PathBuf {
        let full = self.0.join(rel);
        fs::create_dir_all(&full).expect("mkdir");
        full
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn expand_fs(source: &str, root: &Path, opts: IncludeOptions<'_>) -> Expanded {
    let resolver = FileSystemResolver::new(root).expect("root exists");
    expand_with(source, &resolver, opts)
}

#[test]
fn filesystem_resolver_resolves_nested_relative_includes_against_the_parent_directory() {
    let tmp = TempDir::new("nested");
    tmp.write("main.crv", "{{ parts/part.crv }}\n");
    tmp.write("parts/part.crv", "{{ chapters/ch.crv }}\n");
    tmp.write("parts/chapters/ch.crv", "{{ sections/leaf.crv }}\n");
    tmp.write("parts/chapters/sections/leaf.crv", "Deep leaf.\n");
    let source = fs::read_to_string(tmp.path().join("main.crv")).unwrap();
    let result = expand_fs(&source, tmp.path(), IncludeOptions::new());
    assert!(result.warnings.is_empty());
    assert!(result.html.contains("<p>Deep leaf.</p>"));
}

#[test]
fn filesystem_resolver_allows_a_dot_dot_path_whose_canonical_target_stays_inside_the_root() {
    let tmp = TempDir::new("dotdot-ok");
    tmp.write("main.crv", "{{ chapters/ch1.crv }}\n");
    tmp.write(
        "chapters/ch1.crv",
        "Chapter one.\n\n{{ ../shared/glossary.crv }}\n",
    );
    tmp.write("shared/glossary.crv", "Glossary body.\n");
    let source = fs::read_to_string(tmp.path().join("main.crv")).unwrap();
    let result = expand_fs(
        &source,
        tmp.path(),
        IncludeOptions::new().with_source_path(tmp.path().join("main.crv").to_string_lossy()),
    );
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    assert!(result.html.contains("<p>Chapter one.</p>"));
    assert!(result.html.contains("<p>Glossary body.</p>"));
}

#[test]
fn filesystem_resolver_keeps_the_single_top_level_root_for_nested_includes() {
    // The chapter reaches a sibling folder: only possible if the root does not
    // re-base to the including file's directory.
    let tmp = TempDir::new("one-root");
    tmp.write("main.crv", "{{ chapters/ch1.crv }}\n");
    tmp.write("chapters/ch1.crv", "{{ ../shared/note.crv }}\n");
    tmp.write("shared/note.crv", "Shared note.\n");
    let source = fs::read_to_string(tmp.path().join("main.crv")).unwrap();
    let result = expand_fs(&source, tmp.path(), IncludeOptions::new());
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    assert!(result.html.contains("<p>Shared note.</p>"));
}

#[test]
fn filesystem_resolver_rejects_a_dot_dot_chain_that_escapes_the_root() {
    let tmp = TempDir::new("dotdot-escape");
    let root = tmp.mkdir("a/b/root");
    tmp.write("secret.crv", "TOP SECRET\n");
    // Driven through the resolver directly: the core parses the "/../" runs of
    // a multi-level dot-dot path as emphasis, so such a directive never forms
    // in source and cannot exercise containment.
    let resolver = FileSystemResolver::new(&root).expect("root exists");
    let ctx = IncludeContext {
        source_path: None,
        stack: &[],
        depth: 0,
    };
    assert!(resolver.resolve("../../../secret.crv", &ctx).is_none());
    assert!(resolver.resolve("../../../etc/passwd", &ctx).is_none());

    // The sibling-directory case stays allowed through the same resolver.
    fs::create_dir_all(root.join("chapters")).unwrap();
    fs::create_dir_all(root.join("shared")).unwrap();
    fs::write(root.join("shared/ok.crv"), "OK BODY\n").unwrap();
    let stack = [root.join("chapters/ch.crv").to_string_lossy().into_owned()];
    let ctx = IncludeContext {
        source_path: None,
        stack: &stack,
        depth: 0,
    };
    let ok = resolver.resolve("../shared/ok.crv", &ctx).expect("allowed");
    assert_eq!(ok.source, "OK BODY\n");
}

#[test]
fn rejects_a_single_level_dot_dot_escape_written_as_a_directive() {
    let tmp = TempDir::new("single-dotdot");
    let root = tmp.mkdir("root");
    tmp.write("secret.crv", "TOP SECRET\n");
    let result = expand_fs("{{ ../secret.crv }}\n", &root, IncludeOptions::new());
    assert_eq!(result.rules(), vec!["include-unresolved"]);
    assert!(!result.html.contains("TOP SECRET"));
}

#[test]
fn reports_a_containment_denied_target_as_an_unresolved_dependency() {
    let tmp = TempDir::new("denied-dep");
    let root = tmp.mkdir("root");
    tmp.write("secret.crv", "TOP SECRET\n");
    tmp.write("root/ok.crv", "Fine.\n");
    let result = expand_fs(
        "{{ ok.crv }}\n\n{{ ../secret.crv }}\n",
        &root,
        IncludeOptions::new(),
    );
    assert_eq!(result.rules(), vec!["include-unresolved"]);
    let ok_real = fs::canonicalize(root.join("ok.crv")).unwrap();
    assert_eq!(
        result.dependencies,
        vec![
            dep(&ok_real.to_string_lossy(), true),
            dep("../secret.crv", false),
        ]
    );
}

#[cfg(unix)]
#[test]
fn filesystem_resolver_rejects_an_escape_through_a_symlinked_directory_component() {
    let tmp = TempDir::new("symlink-dir");
    let root = tmp.mkdir("root");
    let outside = tmp.mkdir("outside");
    tmp.write("outside/secret.crv", "TOP SECRET\n");
    std::os::unix::fs::symlink(&outside, root.join("linkdir")).unwrap();
    let result = expand_fs("{{ linkdir/secret.crv }}\n", &root, IncludeOptions::new());
    assert_eq!(result.rules(), vec!["include-unresolved"]);
    assert!(!result.html.contains("TOP SECRET"));
}

#[cfg(unix)]
#[test]
fn filesystem_resolver_rejects_symlink_and_dot_dot_escapes_from_the_root() {
    let tmp = TempDir::new("symlink-file");
    let root = tmp.mkdir("root");
    let secret = tmp.write("secret.crv", "TOP SECRET\n");
    std::os::unix::fs::symlink(&secret, root.join("link.crv")).unwrap();
    let result = expand_fs(
        "{{ link.crv }}\n\n{{ ../secret.crv }}\n",
        &root,
        IncludeOptions::new(),
    );
    assert_eq!(
        result.rules(),
        vec!["include-unresolved", "include-unresolved"]
    );
    assert!(!result.html.contains("TOP SECRET"));
}

/// I7 (SECURITY): a warning message is PROCESSOR-generated and names the
/// failure class plus the path AS WRITTEN - never a resolver's raw error text.
///
/// A filesystem resolver knows absolute paths, and its native error strings
/// embed them. Surfacing one verbatim would leak host filesystem layout into
/// output a hosted preview may render, so the message must stay independent of
/// whatever the resolver knows.
///
/// rs is structurally immune - `IncludeResolver::resolve` returns
/// `Option<IncludeResolved>`, so there is no error channel for text to arrive
/// on at all - but that is a property of the current signature, not a
/// guarantee. This pins the observable rule so adding an error channel later
/// cannot quietly start leaking through it.
#[test]
fn a_resolver_failure_never_leaks_host_paths_into_the_warning_message() {
    let tmp = TempDir::new("no-leak");
    let root = tmp.mkdir("root");
    tmp.write("secret.crv", "TOP SECRET\n");
    // Both directives are written RELATIVELY, but the resolver canonicalizes
    // them into absolute paths under the temp root before denying them - a
    // containment escape and a plain missing file. Any absolute path in the
    // resulting message could only have come from the resolver.
    let source = "{{ ../secret.crv }}\n\n{{ gone.crv }}\n";
    let result = expand_fs(source, &root, IncludeOptions::new());
    assert_eq!(
        result.rules(),
        vec!["include-unresolved", "include-unresolved"]
    );

    let root_str = root.to_string_lossy().into_owned();
    let tmp_str = tmp.path().to_string_lossy().into_owned();
    for message in &result.messages {
        assert!(
            !message.contains(&root_str) && !message.contains(&tmp_str),
            "warning message leaked a host path: {message:?}"
        );
        // The path AS WRITTEN is still named, so the author can act on it.
        assert!(
            message.starts_with("Include \""),
            "warning message is not the processor's own: {message:?}"
        );
    }
    // The path AS WRITTEN is echoed back - that is the author's own text and
    // is what makes the warning actionable.
    assert!(result.messages[0].contains("../secret.crv"));
    assert!(result.messages[1].contains("gone.crv"));
    assert!(!result.html.contains("TOP SECRET"));
}

#[test]
fn filesystem_resolver_rejects_an_absolute_path_outside_the_root_by_default() {
    let tmp = TempDir::new("absolute");
    let root = tmp.mkdir("root");
    let secret = tmp.write("secret.crv", "TOP SECRET\n");
    let source = format!("{{{{ \"{}\" }}}}\n", secret.to_string_lossy());
    let result = expand_fs(&source, &root, IncludeOptions::new());
    assert_eq!(result.rules(), vec!["include-unresolved"]);
    assert!(!result.html.contains("TOP SECRET"));
}

#[test]
fn filesystem_resolver_denies_a_missing_target_rather_than_skipping_containment() {
    // `canonicalize` fails on a path that does not exist yet. The error path
    // must DENY: a resolver that fell back to the uncanonicalized candidate
    // would read through symlinks it never checked.
    let tmp = TempDir::new("missing");
    let root = tmp.mkdir("root");
    let resolver = FileSystemResolver::new(&root).expect("root exists");
    let ctx = IncludeContext {
        source_path: None,
        stack: &[],
        depth: 0,
    };
    assert!(resolver.resolve("does-not-exist.crv", &ctx).is_none());
    assert!(resolver.resolve("nested/missing/file.crv", &ctx).is_none());
}

#[test]
fn filesystem_resolver_does_not_confuse_a_sibling_directory_with_a_shared_prefix() {
    // A lexical string-prefix containment test would accept "rootother" as
    // being inside "root"; the component-wise check does not.
    let tmp = TempDir::new("prefix");
    let root = tmp.mkdir("root");
    tmp.write("rootother/secret.crv", "TOP SECRET\n");
    let resolver = FileSystemResolver::new(&root).expect("root exists");
    let ctx = IncludeContext {
        source_path: None,
        stack: &[],
        depth: 0,
    };
    assert!(resolver.resolve("../rootother/secret.crv", &ctx).is_none());
}

#[test]
fn filesystem_resolver_allows_a_directory_whose_name_merely_starts_with_dots() {
    // A `starts_with("..")` prefix test on the relative path would misread
    // "..foo" as an escape.
    let tmp = TempDir::new("dotname");
    let root = tmp.mkdir("root");
    tmp.write("root/..foo/body.crv", "Dotted dir body.\n");
    let result = expand_fs("{{ ..foo/body.crv }}\n", &root, IncludeOptions::new());
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    assert!(result.html.contains("<p>Dotted dir body.</p>"));
}

// ---------------------------------------------------------------------------
// Coverage keyed to the normative rules (spec §19, I1-I11)
// ---------------------------------------------------------------------------

mod rules {
    use super::*;

    #[test]
    fn i1_syntax_a_malformed_value_on_a_known_option_warns_and_stays_literal() {
        let source = "{{ child @shift:x }}";
        let result = expand(source, &[("child", "body")]);
        assert_eq!(result.rules(), vec!["include-unknown-option"]);
        assert_eq!(result.html, literal_html(source));
    }

    #[test]
    fn i1_syntax_an_inverted_line_range_warns_and_stays_literal() {
        let source = "{{ child @lines:3-1 }}";
        let result = expand(source, &[("child", "a\nb\nc")]);
        assert_eq!(result.rules(), vec!["include-unknown-option"]);
        assert_eq!(result.html, literal_html(source));
    }

    #[test]
    fn i1_syntax_a_zero_based_line_range_is_malformed() {
        let source = "{{ child @lines:0-2 }}";
        let result = expand(source, &[("child", "a\nb\nc")]);
        assert_eq!(result.rules(), vec!["include-unknown-option"]);
        assert_eq!(result.html, literal_html(source));
    }

    #[test]
    fn i1_syntax_a_bare_path_stops_at_the_option_marker() {
        let result = expand("{{ child @shift:1 }}", &[("child", "# A")]);
        assert!(result.warnings.is_empty());
        assert!(result.html.contains("<h2>A</h2>"));
    }

    #[test]
    fn i2_block_vs_inline_a_directive_alone_on_a_line_merges_as_blocks() {
        let result = expand("{{ child }}", &[("child", "# Head\n\nBody.")]);
        assert!(result.warnings.is_empty());
        assert!(result.html.contains("<h1>Head</h1>"));
        assert!(result.html.contains("<p>Body.</p>"));
    }

    #[test]
    fn i2_block_vs_inline_a_directive_inside_a_sentence_merges_as_inline() {
        let result = expand("Before {{ child }} after.", &[("child", "middle")]);
        assert!(result.warnings.is_empty());
        assert_eq!(result.html, "<p>Before middle after.</p>");
    }

    #[test]
    fn i3_resolver_model_no_resolver_attempts_no_resolution_at_all() {
        let source = "See {{ child }}.";
        let result = expand_includes(parse(source), source, &IncludeOptions::new());
        assert!(result.warnings.is_empty());
        assert!(result.dependencies.is_empty());
        assert_eq!(render_html(&result.doc), "<p>See {{ child }}.</p>");
    }

    #[test]
    fn i3_resolver_model_a_shielded_directive_is_never_handed_to_the_resolver() {
        let source = "`{{ child }}`\n\n```txt\n{{ child }}\n```";
        let resolver = MapResolver::new(&[("child", "expanded")]);
        let _ = expand_with(source, &resolver, IncludeOptions::new());
        assert!(
            resolver.calls.borrow().is_empty(),
            "resolver saw {:?}",
            resolver.calls.borrow()
        );
    }

    #[test]
    fn i5_collisions_reference_definition_labels_resolve_per_file_without_renaming() {
        // Reference definitions are FILE-LOCAL: resolved inside their own
        // document before the merge, so a label reused by parent and child
        // keeps each file pointing at its own target, with no rename warning.
        let result = expand(
            "Parent [p][ref].\n\n[ref]: https://parent.example\n\n{{ child }}",
            &[("child", "Child [c][ref].\n\n[ref]: https://child.example")],
        );
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
        assert!(result.html.contains("href=\"https://parent.example\""));
        assert!(result.html.contains("href=\"https://child.example\""));
    }

    #[test]
    fn i6_limits_a_file_including_itself_is_caught_as_a_cycle() {
        let result = expand("{{ a }}", &[("a", "Self.\n\n{{ a }}")]);
        assert_eq!(result.rules(), vec!["include-cycle"]);
        assert!(result.html.contains("{{ a }}"));
    }

    #[test]
    fn i7_errors_binary_content_warns_and_stays_literal() {
        let source = "{{ child }}";
        let result = expand(source, &[("child", "binary\u{0}payload")]);
        assert_eq!(result.rules(), vec!["include-non-text"]);
        assert_eq!(result.html, literal_html(source));
        assert!(!result.html.contains("payload"));
    }

    #[test]
    fn i8_shift_a_negative_shift_raises_heading_levels() {
        let result = expand("{{ child @shift:-1 }}", &[("child", "## A\n\n### B")]);
        assert!(result.warnings.is_empty());
        assert!(result.html.contains("<h1>A</h1>"));
        assert!(result.html.contains("<h2>B</h2>"));
    }

    #[test]
    fn i8_shift_clamps_at_level_1_warns_and_keeps_the_heading() {
        let result = expand("{{ child @shift:-2 }}", &[("child", "# A")]);
        assert_eq!(result.rules(), vec!["include-heading-clamp"]);
        assert!(result.html.contains("<h1>A</h1>"));
    }

    #[test]
    fn i8_shift_ids_and_slugs_unchanged_so_a_crossref_into_a_shifted_heading_resolves() {
        let result = expand(
            "{{ child @shift:2 }}",
            &[("child", "# Alpha\n\nSee </#Alpha>.")],
        );
        assert!(result.warnings.is_empty());
        assert!(result.html.contains("<h3>Alpha</h3>"));
        assert!(result.html.contains("id=\"Alpha\""));
        assert!(result.html.contains("href=\"#Alpha\""));
    }

    #[test]
    fn i8_auto_no_preceding_heading_gives_c0_and_leaves_levels_alone() {
        let result = expand("{{ child @shift:auto }}", &[("child", "# Top\n\n## Sub")]);
        assert!(result.warnings.is_empty());
        assert!(result.html.contains("<h1>Top</h1>"));
        assert!(result.html.contains("<h2>Sub</h2>"));
    }

    #[test]
    fn i8_auto_c2_with_child_top_level_1_shifts_by_2() {
        let result = expand(
            "# One\n\n## Two\n\n{{ child @shift:auto }}",
            &[("child", "# Top\n\n## Sub")],
        );
        assert!(result.warnings.is_empty());
        assert!(result.html.contains("<h3>Top</h3>"));
        assert!(result.html.contains("<h4>Sub</h4>"));
    }

    #[test]
    fn i8_auto_uses_the_minimum_child_level_not_the_first_heading() {
        // Child starts at h3 but contains an h2; T is the minimum, so the h2
        // becomes the child's top and the internal gap is preserved.
        let result = expand(
            "# One\n\n{{ child @shift:auto }}",
            &[("child", "### Deep\n\n## Shallow")],
        );
        assert!(result.warnings.is_empty());
        assert!(result.html.contains("<h3>Deep</h3>"));
        assert!(result.html.contains("<h2>Shallow</h2>"));
    }

    #[test]
    fn i8_auto_child_without_headings_is_a_noop_and_warns_about_nothing() {
        let result = expand(
            "# One\n\n## Two\n\n{{ child @shift:auto }}",
            &[("child", "Just a paragraph.")],
        );
        assert!(result.warnings.is_empty());
        assert!(result.html.contains("<p>Just a paragraph.</p>"));
    }

    #[test]
    fn i8_auto_composes_with_section_using_the_selected_subtree_top_level() {
        let result = expand(
            "# One\n\n## Two\n\n{{ child #pick @shift:auto }}",
            &[("child", "# Skipped\n\n{#pick}\n## Picked\n\n### Under")],
        );
        assert!(result.warnings.is_empty());
        assert!(result.html.contains("<h3>Picked</h3>"));
        assert!(result.html.contains("<h4>Under</h4>"));
        assert!(!result.html.contains("Skipped"));
    }

    #[test]
    fn i8_auto_a_closed_sibling_container_does_not_set_the_context_level() {
        // The h2 lives inside a blockquote that has closed by the time the
        // directive is reached, so C falls back to the enclosing h1.
        let result = expand(
            "# One\n\n> ## Quoted\n\n{{ child @shift:auto }}",
            &[("child", "# Top")],
        );
        assert!(result.warnings.is_empty());
        assert!(result.html.contains("<h2>Top</h2>"));
    }

    #[test]
    fn i8_auto_an_enclosing_container_heading_does_set_the_context_level() {
        let result = expand(
            "# One\n\n::: note\n## Inner\n\n{{ child @shift:auto }}\n:::",
            &[("child", "# Top")],
        );
        assert!(result.warnings.is_empty());
        assert!(result.html.contains("<h3 id=\"Top\">Top</h3>"));
    }

    #[test]
    fn i8_auto_resolves_against_the_document_as_assembled_under_a_numeric_parent_shift() {
        // The parent include shifts the child by 1, so the child h1 lands at
        // h2; the grandchild's auto must land one level below that, at h3.
        let result = expand(
            "{{ child @shift:1 }}",
            &[
                ("child", "# ChildTop\n\n{{ grand @shift:auto }}"),
                ("grand", "# GrandTop"),
            ],
        );
        assert!(result.warnings.is_empty());
        assert!(result.html.contains("<h2>ChildTop</h2>"));
        assert!(result.html.contains("<h3>GrandTop</h3>"));
    }

    #[test]
    fn i8_auto_counts_headings_a_child_contributes_only_through_a_nested_include() {
        // The child has no headings of its own; everything comes from the
        // grandchild. Measuring BEFORE expansion would see none and no-op,
        // leaving the grandchild h1 under an h2.
        let result = expand(
            "# One\n\n## Two\n\n{{ child @shift:auto }}",
            &[
                ("child", "{{ grand }}"),
                ("grand", "# GrandTop\n\n## GrandSub"),
            ],
        );
        assert!(result.warnings.is_empty());
        assert!(result.html.contains("<h3>GrandTop</h3>"));
        assert!(result.html.contains("<h4>GrandSub</h4>"));
    }

    #[test]
    fn i8_auto_a_stated_parent_shift_still_places_a_nested_auto_by_the_assembled_level() {
        // The child is shifted by 1 explicitly and has no headings of its own,
        // so the grandchild's auto must key off the parent h1 AS ASSEMBLED,
        // landing at h2 rather than being pushed twice.
        let result = expand(
            "# One\n\n{{ child @shift:1 }}",
            &[
                ("child", "{{ grand @shift:auto }}"),
                ("grand", "# GrandTop"),
            ],
        );
        assert!(result.warnings.is_empty());
        assert!(result.html.contains("<h2>GrandTop</h2>"));
    }

    #[test]
    fn i8_auto_a_heading_merged_by_an_earlier_include_sets_the_context_for_a_later_one() {
        let result = expand(
            "{{ first }}\n\n{{ second @shift:auto }}",
            &[("first", "# First\n\n## Deeper"), ("second", "# SecondTop")],
        );
        assert!(result.warnings.is_empty());
        assert!(result.html.contains("<h3>SecondTop</h3>"));
    }

    #[test]
    fn i8_auto_is_a_noop_for_an_inline_include_whose_content_has_no_headings() {
        let result = expand(
            "# One\n\n## Two\n\nSee {{ child @shift:auto }} here.",
            &[("child", "a fragment")],
        );
        assert!(result.warnings.is_empty());
        assert!(result.html.contains("<p>See a fragment here.</p>"));
    }

    #[test]
    fn i9_verbatim_a_raw_block_keeps_a_directive_literal() {
        let result = expand("```=html\n{{ child }}\n```", &[("child", "EXPANDED")]);
        assert!(result.warnings.is_empty());
        assert!(result.html.contains("{{ child }}"));
        assert!(!result.html.contains("EXPANDED"));
    }

    #[test]
    fn i9_verbatim_a_fence_with_an_info_string_shields_a_plain_directive_still_expands() {
        let result = expand(
            "```js\n{{ child }}\n```\n\n{{ child }}",
            &[("child", "EXPANDED")],
        );
        assert!(result.warnings.is_empty());
        assert!(result
            .html
            .contains("<code class=\"language-js\">{{ child }}"));
        assert!(result.html.contains("<p>EXPANDED</p>"));
    }

    #[test]
    fn i11_dependencies_report_a_cycle_broken_target_as_attempted() {
        let result = expand("{{ a }}", &[("a", "Self.\n\n{{ a }}")]);
        assert_eq!(result.rules(), vec!["include-cycle"]);
        // "a" resolved once at the top level; the cycle-broken second attempt
        // does not demote it, and no phantom entry appears.
        assert_eq!(result.dependencies, vec![dep("a", true)]);
    }

    #[test]
    fn i11_dependencies_report_a_depth_exceeded_target_as_attempted() {
        let result = expand_opts(
            "{{ a }}",
            &[("a", "{{ b }}"), ("b", "done")],
            IncludeOptions::new().with_max_depth(1),
        );
        assert_eq!(result.rules(), vec!["include-depth"]);
        assert_eq!(result.dependencies, vec![dep("a", true), dep("b", false)]);
    }

    #[test]
    fn i11_dependencies_report_a_binary_target_as_attempted() {
        let result = expand("{{ child }}", &[("child", "binary\u{0}payload")]);
        assert_eq!(result.dependencies, vec![dep("child", false)]);
    }

    // -- Warning file attribution (I4/I7) --

    #[test]
    fn attributes_an_unresolvable_directive_to_the_top_level_document() {
        let result = expand_opts(
            "{{ missing }}",
            &[],
            IncludeOptions::new().with_source_path("book.crv"),
        );
        assert_eq!(
            result.warnings,
            vec![(
                "include-unresolved".to_string(),
                Some("book.crv".to_string())
            )]
        );
    }

    #[test]
    fn attributes_a_warning_raised_while_expanding_a_child_to_the_child() {
        // The clamp happens on a heading that lives in child.crv, even though
        // the directive that pulled it in lives in the parent.
        let result = expand_opts(
            "{{ child.crv @shift:1 }}",
            &[("child.crv", "###### Deep")],
            IncludeOptions::new().with_source_path("book.crv"),
        );
        assert_eq!(
            result.warnings,
            vec![(
                "include-heading-clamp".to_string(),
                Some("child.crv".to_string())
            )]
        );
    }

    #[test]
    fn attributes_a_grandchild_warning_to_the_grandchild_not_an_ancestor() {
        // Only the innermost file has a directive that fails, so attribution
        // must walk the whole chain rather than stopping at the root or at the
        // file that owns the outermost include.
        let result = expand_opts(
            "{{ chapter.crv }}",
            &[
                ("chapter.crv", "Chapter.\n\n{{ section.crv }}"),
                ("section.crv", "Section.\n\n{{ missing.crv }}"),
            ],
            IncludeOptions::new().with_source_path("book.crv"),
        );
        assert_eq!(
            result.warnings,
            vec![(
                "include-unresolved".to_string(),
                Some("section.crv".to_string())
            )]
        );
    }

    #[test]
    fn omits_the_file_entirely_when_the_top_level_document_has_no_source_path() {
        let result = expand("{{ missing }}", &[]);
        assert_eq!(
            result.warnings,
            vec![("include-unresolved".to_string(), None)]
        );
    }
}

// ---------------------------------------------------------------------------
// I7: a rejected directive has NO observable side effects
// ---------------------------------------------------------------------------

/// A rejected directive must leave the document byte-identical to one where it
/// had been written as literal text from the start, and must release every
/// identifier it claimed while the child was being processed.
///
/// The second half is what these guard: identifier reservations are made
/// DURING child processing, before anything knows whether the content can
/// actually land. A leak is invisible in the rejected directive's own output -
/// it surfaces only in a LATER include, which silently gets `-2` appended to an
/// id or footnote label that was never really taken.
mod rejected_directives_have_no_side_effects {
    use super::*;

    fn tuned(
        source: &str,
        files: &[(&str, &str)],
        max_depth: Option<usize>,
        max_bytes: Option<usize>,
    ) -> Expanded {
        let mut opts = IncludeOptions::new();
        if let Some(depth) = max_depth {
            opts = opts.with_max_depth(depth);
        }
        if let Some(bytes) = max_bytes {
            opts = opts.with_max_bytes(bytes);
        }
        expand_opts(source, files, opts)
    }

    /// Probe 1: the rejected directive ALONE renders exactly as the core does
    /// with no resolver configured - it really did degrade to literal text.
    fn assert_literal(source: &str, result: &Expanded) {
        assert_eq!(
            result.html,
            literal_html(source),
            "rejected directive did not render as plain literal text"
        );
    }

    /// A child that claims `dup` and merges successfully, used as the survivor
    /// in every "the reservation was released" probe below.
    const GOOD: &str = "{#dup}\n# Good";

    /// Probe 2: a SUCCESSFUL include after a rejected one keeps the explicit id
    /// the rejected child had also claimed.
    fn assert_id_released(result: &Expanded) {
        assert!(
            result.html.contains("id=\"dup\""),
            "later include lost the id a rejected directive had reserved: {}",
            result.html
        );
        assert!(
            !result.html.contains("id=\"dup-2\""),
            "rejected directive kept its heading id reserved: {}",
            result.html
        );
        assert!(
            !result.rules().contains(&"include-heading-id-rename"),
            "rejected directive caused a spurious rename: {:?}",
            result.rules()
        );
    }

    // -- the found case ----------------------------------------------------

    #[test]
    fn rejected_inline_block_include_releases_the_heading_ids_it_reserved() {
        // The inline directive resolves, so its explicit ids get claimed, and
        // only THEN is the content found to be block-level and rejected.
        let result = expand(
            "See {{ blocky }} here.\n\n{{ good }}",
            &[("blocky", "{#dup}\n# One\n\nTwo."), ("good", GOOD)],
        );
        assert!(result.rules().contains(&"include-block-in-inline"));
        assert!(result.html.contains("<p>See {{ blocky }} here.</p>"));
        assert_id_released(&result);
    }

    #[test]
    fn rejected_inline_block_include_releases_the_footnote_labels_it_reserved() {
        let result = expand(
            "See {{ blocky }} here.\n\n{{ good }}",
            &[
                ("blocky", "One[^n]\n\nTwo.\n\n[^n]: Rejected body."),
                ("good", "Kept[^n]\n\n[^n]: Kept body."),
            ],
        );
        assert!(result.rules().contains(&"include-block-in-inline"));
        assert!(
            !result.rules().contains(&"include-footnote-rename"),
            "rejected directive kept its footnote label reserved: {:?}",
            result.rules()
        );
        assert!(result.html.contains("Kept body."), "html: {}", result.html);
        assert!(
            !result.html.contains("Rejected body."),
            "rejected child's footnote body leaked into the output: {}",
            result.html
        );
    }

    #[test]
    fn a_rejection_releases_what_its_successful_nested_includes_reserved() {
        // The OUTER directive is rejected for block content, but its child had
        // already expanded a nested include that claimed `dup`. Rolling back
        // only the outer frame would not be enough.
        let result = expand(
            "See {{ blocky }} here.\n\n{{ good }}",
            &[
                ("blocky", "{{ nested }}\n\nSecond block."),
                ("nested", "{#dup}\n# Nested"),
                ("good", GOOD),
            ],
        );
        assert!(result.rules().contains(&"include-block-in-inline"));
        assert_id_released(&result);
    }

    // -- one probe pair per rejection reason --------------------------------

    #[test]
    fn unresolvable_target_has_no_side_effects() {
        let source = "{{ missing }}";
        let result = expand(source, &[]);
        assert!(result.rules().contains(&"include-unresolved"));
        assert_literal(source, &result);

        let later = expand("{{ missing }}\n\n{{ good }}", &[("good", GOOD)]);
        assert_id_released(&later);
    }

    #[test]
    fn binary_content_has_no_side_effects() {
        let source = "{{ blob }}";
        let result = expand(source, &[("blob", "{#dup}\n# X\0binary")]);
        assert!(result.rules().contains(&"include-non-text"));
        assert_literal(source, &result);

        let later = expand(
            "{{ blob }}\n\n{{ good }}",
            &[("blob", "{#dup}\n# X\0binary"), ("good", GOOD)],
        );
        assert_id_released(&later);
    }

    #[test]
    fn both_selections_present_has_no_side_effects() {
        let source = "{{ child #sec @lines:1-2 }}";
        let files: &[(&str, &str)] = &[("child", "{#dup}\n# Sec")];
        let result = expand(source, files);
        assert!(result.rules().contains(&"include-selection-conflict"));
        assert_literal(source, &result);

        let later = expand(
            "{{ child #sec @lines:1-2 }}\n\n{{ good }}",
            &[("child", "{#dup}\n# Sec"), ("good", GOOD)],
        );
        assert_id_released(&later);
    }

    #[test]
    fn missing_section_has_no_side_effects() {
        let source = "{{ child #nope }}";
        let files: &[(&str, &str)] = &[("child", "{#dup}\n# Sec")];
        let result = expand(source, files);
        assert!(result.rules().contains(&"include-section"));
        assert_literal(source, &result);

        let later = expand(
            "{{ child #nope }}\n\n{{ good }}",
            &[("child", "{#dup}\n# Sec"), ("good", GOOD)],
        );
        assert_id_released(&later);
    }

    #[test]
    fn cycle_has_no_side_effects() {
        let source = "{{ a }}";
        let files: &[(&str, &str)] = &[("a", "{{ a }}")];
        let result = expand(source, files);
        assert!(result.rules().contains(&"include-cycle"));

        // The OUTER `a` merges and legitimately keeps `dup`; the inner,
        // cycle-rejected re-entry must not claim it a second time and rename
        // its own copy to `dup-2`.
        let later = expand("{{ a }}", &[("a", "{#dup}\n# A\n\n{{ a }}")]);
        assert!(later.rules().contains(&"include-cycle"));
        assert!(later.html.contains("id=\"dup\""), "html: {}", later.html);
        assert!(
            !later.html.contains("id=\"dup-2\""),
            "cycle-rejected re-entry reserved a duplicate id: {}",
            later.html
        );
    }

    #[test]
    fn depth_limit_has_no_side_effects() {
        let source = "{{ a }}";
        let files: &[(&str, &str)] = &[("a", "{{ b }}"), ("b", "{#dup}\n# B")];
        // Depth 0 rejects the directive itself, so the document stays literal.
        let result = tuned(source, files, Some(0), None);
        assert!(result.rules().contains(&"include-depth"));
        assert_literal(source, &result);

        // Depth 1 lets `a` in and rejects its nested `b`, so `a` merges as an
        // empty passthrough while `b`'s ids must never have been claimed.
        let later = tuned(
            "{{ a }}\n\n{{ good }}",
            &[("a", "{{ b }}"), ("b", "{#dup}\n# B"), ("good", GOOD)],
            Some(1),
            None,
        );
        assert!(later.rules().contains(&"include-depth"));
        assert_id_released(&later);
    }

    #[test]
    fn size_limit_has_no_side_effects() {
        let big = format!("{{#dup}}\n# Big\n\n{}", "x".repeat(4096));
        let source = "{{ big }}";
        let files: &[(&str, &str)] = &[("big", big.as_str())];
        // Large enough for `good`, far too small for `big`.
        let result = tuned(source, files, None, Some(64));
        assert!(result.rules().contains(&"include-budget"));
        assert_literal(source, &result);

        let later = tuned(
            "{{ big }}\n\n{{ good }}",
            &[("big", big.as_str()), ("good", GOOD)],
            None,
            Some(64),
        );
        assert_id_released(&later);
    }

    #[test]
    fn block_content_in_inline_position_has_no_side_effects() {
        let source = "See {{ blocky }} here.";
        let files: &[(&str, &str)] = &[("blocky", "{#dup}\n# One\n\nTwo.")];
        let result = expand(source, files);
        assert!(result.rules().contains(&"include-block-in-inline"));
        assert_literal(source, &result);
    }
}
