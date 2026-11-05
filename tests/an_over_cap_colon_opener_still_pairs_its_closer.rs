//! Past the nesting cap a colon opener is paragraph text (PART 9 §25), but it
//! still pairs with its closer, as in the spec model's closer walk. Each body
//! walk used to cap its fence stack from its own opener, so walks started at
//! different depths paired the same closers differently (carve-rs#3891). The
//! HTML below is the spec model's.

use carve::{render_carve, to_html};

fn on_big_stack<F: FnOnce() + Send + 'static>(f: F) {
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(f)
        .expect("spawn worker")
        .join()
        .expect("worker");
}

/// `outer` opens first, then `inner` openers of width three, then `phantoms`
/// over-cap `::: note` lines, `lits` bare `:::` lines and every closer.
fn ladder(outer: usize, inner: usize, phantoms: usize, lits: usize) -> String {
    let mut s = format!("{} note\n", ":".repeat(outer));
    s += &"::: note\n".repeat(inner);
    s += &"::: note\n\n".repeat(phantoms);
    s += &":::\n".repeat(lits + inner);
    s += &format!("{}\n", ":".repeat(outer));
    s
}

fn body(html: &str) -> Vec<String> {
    html.lines()
        .map(str::trim)
        .filter(|line| !line.contains("aside"))
        .map(str::to_owned)
        .collect()
}

#[test]
fn an_over_cap_opener_takes_its_closer() {
    on_big_stack(|| {
        let html = to_html(&ladder(4, 199, 1, 0));
        assert_eq!(html.matches("<aside").count(), 200);
        assert_eq!(
            body(&html),
            ["<p>::: note</p>", "<p>:::</p>", "<div>", "", "</div>"]
        );
    });
}

#[test]
fn widened_fences_read_back_after_fmt() {
    on_big_stack(|| {
        let mut source = String::new();
        for _ in 0..202 {
            source += "::: note\n\n";
        }
        source += &":::\n".repeat(200);
        let html = to_html(&source);
        let written = render_carve(&carve::parse(&source)).unwrap();
        assert_eq!(to_html(&written), html);
    });
}

#[test]
fn a_ladder_under_the_cap_is_unchanged() {
    on_big_stack(|| {
        let html = to_html(&ladder(4, 198, 0, 0));
        assert_eq!(html.matches("<aside").count(), 199);
        assert!(body(&html).iter().all(String::is_empty));
    });
}
