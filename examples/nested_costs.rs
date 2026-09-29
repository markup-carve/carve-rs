//! Run with `cargo run --release --example nested_costs`.
use std::time::{Duration, Instant};

fn main() {
    for (family, marker) in [("quote", "> "), ("list", "- ")] {
        for depth in [48, 96, 192] {
            let source = format!("{}end\n", marker.repeat(depth));
            let doc = carve::parse(&source);
            let expected = carve::render_html(&doc).unwrap();
            assert_eq!(expected, carve::to_html(&source));
            for mode in ["parse", "render", "html"] {
                let run = || match mode {
                    "parse" => drop(std::hint::black_box(carve::parse(&source))),
                    "render" => drop(std::hint::black_box(carve::render_html(&doc).unwrap())),
                    _ => drop(std::hint::black_box(carve::to_html(&source))),
                };
                let start = Instant::now();
                while start.elapsed() < Duration::from_millis(150) {
                    run();
                }
                let mut samples = Vec::new();
                for _ in 0..7 {
                    let start = Instant::now();
                    let mut calls = 0;
                    loop {
                        run();
                        calls += 1;
                        if start.elapsed() >= Duration::from_millis(50) {
                            break;
                        }
                    }
                    samples.push(start.elapsed().as_secs_f64() * 1000.0 / f64::from(calls));
                }
                println!(
                    "{}",
                    serde_json::json!({"family":family,"depth":depth,"mode":mode,
                        "input_bytes":source.len(),"html_bytes":expected.len(),"samples_ms":samples})
                );
            }
        }
    }
}
