//! Emphasis the parser reads under deep containers is written back, not
//! refused: the writer refused whenever block and inline depth together
//! reached the cap, but the parser reads paragraph inlines on their own budget
//! (CARVE-P9-078, carve-rs#3892).

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
fn emphasis_under_deep_fences_reads_back() {
    on_big_stack(|| {
        for depth in [198, 199, 200, 201, 230] {
            for closers in [0, depth] {
                let mut source = "::: note\n_em_ and *strong* text\n".repeat(depth);
                source += &":::\n".repeat(closers);
                let written = render_carve(&carve::parse(&source))
                    .unwrap_or_else(|error| panic!("{depth}/{closers}: {error}"));
                assert_eq!(to_html(&written), to_html(&source), "{depth}/{closers}");
                assert_eq!(
                    render_carve(&carve::parse(&written)).unwrap(),
                    written,
                    "{depth}/{closers}: idempotent"
                );
            }
        }
    });
}
