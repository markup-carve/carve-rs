fn main() {
    for family in ["paragraph", "blocks"] {
        for n in [500, 1000, 2000, 4000] {
            let source = if family == "paragraph" {
                format!("a\n{}", "{% x\ny %}\n".repeat(n))
            } else {
                "{% x\n  y %}\n".repeat(n)
            };
            let mut samples = Vec::new();
            for _ in 0..3 {
                let start = std::time::Instant::now();
                let out = carve::djot_to_carve(&source);
                assert!(!out.contains("{% x"));
                samples.push(start.elapsed().as_millis());
            }
            println!("{family} {n} {samples:?}");
        }
    }
}
