//! Containment and limits of include expansion with the file resolver
//! (C2-N-011 to C2-N-014, C2-N-064 to C2-N-068).
use carve2::include::{Includes, fs};
use carve2::options::Options;
use std::path::{Path, PathBuf};

struct Tree {
    base: PathBuf,
}

impl Tree {
    fn new(name: &str) -> Self {
        let base = std::env::temp_dir().join(format!("carve2-sec-{}-{name}", std::process::id()));
        std::fs::create_dir_all(base.join("root")).unwrap();
        std::fs::create_dir_all(base.join("outside")).unwrap();
        std::fs::write(base.join("outside/secret.crv"), "TOP SECRET\n").unwrap();
        Tree {
            base: std::fs::canonicalize(&base).unwrap(),
        }
    }

    fn file(&self, rel: &str, bytes: &[u8]) -> &Self {
        let p = self.base.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, bytes).unwrap();
        self
    }

    fn root(&self) -> PathBuf {
        self.base.join("root")
    }

    fn render(&self, entry: &str) -> carve2::Rendered {
        let mut inc = Includes::new(fs::resolver(&self.root()).unwrap());
        inc.file = Some("main.crv".into());
        let opts = Options {
            includes: Some(inc),
            ..Options::default()
        };
        carve2::render_with_diagnostics(entry, &opts)
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

/// C2-N-068: no host path in any message, and nothing of the secret in the output.
fn assert_sealed(t: &Tree, out: &carve2::Rendered) {
    let host = t.base.display().to_string();
    assert!(!out.html.contains("TOP SECRET"), "{}", out.html);
    for d in &out.diagnostics {
        assert!(!d.message.contains(&host), "{}", d.message);
    }
}

fn codes(out: &carve2::Rendered) -> Vec<String> {
    out.codes()
        .into_iter()
        .filter(|c| c != "include-unresolved")
        .collect()
}

#[test]
fn c2_n_012_traversal_out_of_the_root_is_refused() {
    let t = Tree::new("traversal");
    for entry in [
        "{{ ../outside/secret.crv }}",
        "{{ a/../../outside/secret.crv }}",
        "{{ ../outside/missing.crv }}",
        "{{ https://x.org/a.crv }}",
    ] {
        let out = t.render(entry);
        assert_eq!(codes(&out), ["include-outside-root"], "{entry}");
        assert_sealed(&t, &out);
    }
    let abs = format!(
        "{{{{ \"{}\" }}}}",
        t.base.join("outside/secret.crv").display()
    );
    let out = t.render(&abs);
    assert_eq!(codes(&out), ["include-outside-root"]);
    assert!(!out.html.contains("TOP SECRET"));
}

#[cfg(unix)]
#[test]
fn c2_n_013_symbolic_links_out_of_the_root_are_refused() {
    let t = Tree::new("links");
    std::os::unix::fs::symlink(t.base.join("outside"), t.root().join("dir")).unwrap();
    std::os::unix::fs::symlink(t.base.join("outside/secret.crv"), t.root().join("file.crv"))
        .unwrap();
    for entry in [
        "{{ dir/secret.crv }}",
        "{{ file.crv }}",
        "{{ dir/missing.crv }}",
    ] {
        let out = t.render(entry);
        assert_eq!(codes(&out), ["include-outside-root"], "{entry}");
        assert_sealed(&t, &out);
    }
    std::os::unix::fs::symlink(t.root().join("inner.crv"), t.root().join("alias.crv")).unwrap();
    t.file("root/inner.crv", b"Inner.\n");
    assert_eq!(t.render("{{ alias.crv }}").html, "<p>Inner.</p>");
}

#[test]
fn c2_n_013_inside_the_root_missing_and_unreadable_are_told_apart() {
    let t = Tree::new("causes");
    std::fs::create_dir_all(t.root().join("dir.crv")).unwrap();
    assert_eq!(codes(&t.render("{{ missing.crv }}")), ["include-not-found"]);
    assert_eq!(codes(&t.render("{{ dir.crv }}")), ["include-unreadable"]);
}

#[test]
fn c2_n_011_binary_content_is_not_text() {
    let t = Tree::new("binary");
    t.file("root/bin.dat", &[0x61, 0xFF, 0x62])
        .file("root/nul.crv", b"a\0b\n");
    for entry in ["{{ bin.dat }}", "{{ nul.crv }}"] {
        let out = t.render(entry);
        assert_eq!(codes(&out), ["include-non-text"], "{entry}");
        assert!(!out.html.contains('\u{FFFD}'));
    }
}

#[test]
fn c2_n_065_a_file_over_the_budget_is_refused() {
    let t = Tree::new("budget");
    let big = vec![b'a'; carve2::tables::INCLUDE_BYTES + 1];
    t.file("root/big.crv", &big).file("root/small.crv", b"S\n");
    let out = t.render("{{ big.crv }}\n\n{{ small.crv }}");
    assert_eq!(codes(&out), ["include-budget"]);
    assert!(
        out.html.ends_with("<p>S</p>"),
        "{}",
        &out.html[out.html.len() - 40..]
    );
}

#[test]
fn c2_n_066_a_cycle_is_read_once_on_its_chain() {
    let t = Tree::new("cycle");
    t.file("root/main.crv", b"{{ a.crv }}\n")
        .file("root/a.crv", b"A\n\n{{ b.crv }}\n")
        .file("root/b.crv", b"B\n\n{{ a.crv }}\n\n{{ main.crv }}\n");
    let out = t.render("{{ a.crv }}");
    assert_eq!(codes(&out), ["include-cycle", "include-cycle"]);
    assert_eq!(out.html.matches("<p>A</p>").count(), 1);
}

#[test]
fn c2_n_064_a_chain_stops_at_the_depth_limit() {
    let t = Tree::new("depth");
    for i in 0..20 {
        t.file(
            &format!("root/f{i}.crv"),
            format!("L{i}\n\n{{{{ f{}.crv }}}}\n", i + 1).as_bytes(),
        );
    }
    let out = t.render("{{ f0.crv }}");
    assert_eq!(codes(&out), ["include-depth"]);
    assert!(out.html.contains("<p>L15</p>") && !out.html.contains("<p>L16</p>"));
}

#[test]
fn c2_n_014_a_root_that_is_not_absolute_is_refused() {
    assert!(fs::resolver(Path::new("relative/root")).is_err());
}
