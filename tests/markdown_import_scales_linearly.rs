//! Markdown import work grows with the input, counted rather than timed (markup-carve/carve-rs#2971).

use carve::{migrate_markdown, take_attribute_opener_scan_bytes};

fn scan_bytes(lines: usize, heading: bool) -> u64 {
    let mut source = String::new();
    let mut line_count = 0;
    for i in 0.. {
        let block = if heading {
            format!(
                "## Heading {i}\n\ntext with [link {i}](http://e.com/{i}) and *em* **strong**\n\n"
            )
        } else {
            format!("text [link {i}](http://e.com/{i})\n\n")
        };
        line_count += block.bytes().filter(|byte| *byte == b'\n').count();
        source.push_str(&block);
        if line_count >= lines {
            break;
        }
    }
    take_attribute_opener_scan_bytes();
    let result = migrate_markdown(&source);
    assert!(!result.value.is_empty());
    take_attribute_opener_scan_bytes()
}

#[test]
fn markdown_import_scales_linearly() {
    for heading in [false, true] {
        let counts = [1000, 4000, 16000].map(|lines| scan_bytes(lines, heading));
        eprintln!("{}: {counts:?}", if heading { "headings" } else { "links" });
        assert!(counts[0] > 0);
        for pair in counts.windows(2) {
            assert!(pair[1] <= pair[0] * 5, "{counts:?}");
        }
    }
}
