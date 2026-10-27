#[test]
fn two_hundred_nested_links_reach_the_cap_without_early_flattening() {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            for depth in [199, 200] {
                let mut source = "x".to_string();
                for _ in 0..depth {
                    source = format!("[{source}](#)");
                }
                assert_eq!(
                    carve::to_html(&source),
                    "<p><a href=\"#\">x</a></p>",
                    "depth {depth}"
                );
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn two_hundred_and_one_nested_links_degrade_at_the_cap() {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            let mut source = "x".to_string();
            for _ in 0..201 {
                source = format!("[{source}](#)");
            }
            assert_eq!(carve::to_html(&source), "<p><a href=\"#\">[x](#)</a></p>");
        })
        .unwrap()
        .join()
        .unwrap();
}
