//! `FileSystemResolver` reads regular files only. A FIFO reports length 0, so
//! the size cap passes it, and `read_to_string` then blocks until a writer
//! opens the pipe (markup-carve/carve-js#3108).

#![cfg(feature = "fs")]

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use carve::{FileSystemResolver, IncludeContext, IncludeDenial, IncludeResolver};

fn temp_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "carve-include-special-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::canonicalize(root).unwrap()
}

fn resolve(root: PathBuf, path: &'static str, cap: Option<u64>) -> Result<String, IncludeDenial> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let resolver = FileSystemResolver::new(&root)
            .unwrap()
            .with_max_file_bytes(cap);
        let ctx = IncludeContext {
            source_path: None,
            stack: &[],
            depth: 0,
        };
        let _ = sender.send(resolver.resolve(path, &ctx).map(|resolved| resolved.source));
    });
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("the resolver blocked instead of refusing")
}

#[cfg(unix)]
#[test]
fn a_fifo_is_denied_without_being_opened() {
    let root = temp_root("fifo");
    let status = std::process::Command::new("mkfifo")
        .arg(root.join("pipe.crv"))
        .status()
        .unwrap();
    assert!(status.success());
    for cap in [Some(4 * 1024 * 1024), None] {
        assert_eq!(
            resolve(root.clone(), "pipe.crv", cap),
            Err(IncludeDenial::Denied)
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_directory_is_denied() {
    let root = temp_root("dir");
    std::fs::create_dir(root.join("chapter.crv")).unwrap();
    assert_eq!(
        resolve(root.clone(), "chapter.crv", None),
        Err(IncludeDenial::Denied)
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_regular_file_still_resolves() {
    let root = temp_root("file");
    std::fs::write(root.join("child.crv"), "Child.\n").unwrap();
    assert_eq!(
        resolve(root.clone(), "child.crv", None),
        Ok("Child.\n".to_owned())
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn an_ill_formed_utf8_file_is_denied_not_replaced() {
    let root = temp_root("ill-formed");
    std::fs::write(root.join("child.crv"), b"a\xffb\n").unwrap();
    assert_eq!(
        resolve(root.clone(), "child.crv", None),
        Err(IncludeDenial::Denied)
    );
    std::fs::remove_dir_all(root).unwrap();
}
