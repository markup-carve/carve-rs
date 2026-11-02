//! carve#4973: the readback check must not count the break marker the writer
//! drops, or a capped document with a `---` break is refused as unspellable.

fn on_big_stack<F: FnOnce() + Send + 'static>(f: F) {
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(f)
        .expect("thread spawns")
        .join()
        .expect("test thread completes");
}

#[test]
fn every_break_spelling_writes_beside_a_list_at_the_parse_cap() {
    on_big_stack(|| {
        let pad = "  ".repeat(200);
        let ladder = (0..200)
            .map(|i| format!("{}- x", "  ".repeat(i)))
            .collect::<Vec<_>>()
            .join("\n")
            + &format!("\n{pad}```\n{pad}code\n{pad}```");
        let mut written = Vec::new();
        for spelling in ["***", "---", "___"] {
            let source = format!("{spelling}\n\n{ladder}\n");
            let out = carve::render_carve(&carve::parse(&source))
                .unwrap_or_else(|error| panic!("{spelling:?} refused: {error}"));
            assert!(out.starts_with("***\n\n- x\n"), "{spelling:?}");
            written.push(out);
        }
        assert!(written.windows(2).all(|w| w[0] == w[1]));
    });
}
