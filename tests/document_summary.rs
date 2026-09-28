use crate::common;
use carve::{BlockNode, BlockQuote, ThematicBreak};

#[test]
fn summary_is_bounded_for_a_deep_document() {
    const CHILD: &str = "CARVE_SUMMARY_TEST_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                &common::exact_test_name(module_path!(), "summary_is_bounded_for_a_deep_document"),
            ])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success() && String::from_utf8_lossy(&output.stdout).contains("1 passed"),
            "stdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            let mut document = carve::parse("private content");
            let mut block = BlockNode::ThematicBreak(ThematicBreak::default());
            for _ in 0..20_000 {
                block = BlockNode::BlockQuote(BlockQuote {
                    attrs: None,
                    children: vec![block],
                    fenced: false,
                    pos: None,
                });
            }
            document.children.push(block);
            document
                .frontmatter
                .insert("private key".into(), "private value".into());
            document
                .footnote_defs
                .insert("private label".into(), Vec::new());
            let summary = document.summary();
            assert_eq!(summary.root_blocks, 2);
            assert_eq!(summary.footnote_definitions, 1);
            assert_eq!(summary.frontmatter_entries, 1);
            assert_eq!(summary.source_bytes, 15);
            let debug = format!("{summary:?}");
            assert!(debug.len() < 256);
        })
        .unwrap()
        .join()
        .unwrap();
}
