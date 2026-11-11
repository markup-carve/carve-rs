//! Erased `%%` lines come back into the tree in linear time (carve-rs#4266,
//! PART 9 §9 LINEAR TIME).

use std::time::{Duration, Instant};

fn best_time(source: &str) -> Duration {
    (0..3)
        .map(|_| {
            let start = Instant::now();
            assert!(!carve::to_html(source).is_empty());
            start.elapsed()
        })
        .min()
        .unwrap()
}

fn paragraph(n: usize) -> String {
    format!("a\n{}z\n", "%%\n".repeat(n))
}

fn item(n: usize) -> String {
    format!("- a\n{}  z\n", "  %%\n".repeat(n))
}

fn quote(n: usize) -> String {
    format!("> a\n{}> z\n", "> %%\n".repeat(n))
}

fn nested_quoted_item(n: usize) -> String {
    format!("> > - a\n{}> >   z\n", "> >   %%\n".repeat(n))
}

#[test]
fn erased_comment_runs_scale_linearly() {
    for shape in [
        paragraph as fn(usize) -> String,
        item,
        quote,
        nested_quoted_item,
    ] {
        let small = best_time(&shape(1000));
        let large = best_time(&shape(4000));
        // 4x the lines: linear is about 4x, the quadratic path was 16x.
        assert!(
            large <= small * 9 + Duration::from_millis(50),
            "1000 lines {small:?}, 4000 lines {large:?}"
        );
    }
}

#[test]
fn erased_comment_runs_keep_their_output() {
    assert_eq!(carve::to_html(&paragraph(3)).trim(), "<p>a\nz</p>");
    assert_eq!(
        carve::to_html(&item(3)).trim(),
        "<ul>\n  <li>a\nz</li>\n</ul>"
    );
    assert_eq!(
        carve::to_html(&quote(3)).trim(),
        "<blockquote><p>a\nz</p></blockquote>"
    );
}
