//! A `:::` paragraph line in the 200th container would open container 201, so
//! it is text; the parser still reads its inlines, so the writer must escape it
//! once fmt adds closers it would pair with (carve-rs#3895).

use carve::{render_carve, to_html};

fn on_big_stack<F: FnOnce() + Send + 'static>(f: F) {
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(f)
        .expect("spawn worker")
        .join()
        .expect("worker");
}

#[test]
fn a_bare_colon_line_past_the_cap_is_written_escaped() {
    on_big_stack(|| {
        for text in [":::", "::: note"] {
            let source = format!("{}:::: note\n\n{text}\n", "::: note\n\n".repeat(199));
            let written = render_carve(&carve::parse(&source)).expect("writes");
            assert!(
                written.lines().any(|line| line == format!("\\{text}")),
                "{text:?} not escaped"
            );
            assert_eq!(to_html(&written), to_html(&source), "{text:?}");
        }
    });
}
