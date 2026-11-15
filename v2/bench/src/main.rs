//! Times the 0.2 engine against the 0.1 engine on the normal path, in one process.
//!
//! `cargo run --release --manifest-path v2/bench/Cargo.toml -- [--rounds N] [--ms N] [--loop SECS] FILE...`
//!
//! Times are CPU time of the thread. Prints, per document: the median time and spread of 0.2 parse, 0.2 parse plus HTML,
//! 0.1 `parse` and 0.1 `render_html(&parse(src))`, measured alternately; then the 0.2
//! stages (block pass, inline pass, the rest of the tree builder, resolve, HTML) with
//! their time, allocation count and allocated bytes.
#![allow(clippy::cast_precision_loss)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::time::{Duration, Instant};

struct Counting;

static ALLOCS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);
/// The hook only records during the stage runs, so the timed rounds stay unhooked.
static HOOKED: AtomicBool = AtomicBool::new(false);

// SAFETY: forwards every call to the system allocator unchanged; the counters are side effects only.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Relaxed);
        BYTES.fetch_add(l.size() as u64, Relaxed);
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        ALLOCS.fetch_add(1, Relaxed);
        BYTES.fetch_add(n as u64, Relaxed);
        unsafe { System.realloc(p, l, n) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

#[derive(Clone, Copy, Default)]
struct Stat {
    ns: u64,
    allocs: u64,
    bytes: u64,
}

thread_local! {
    static INLINE: Cell<Stat> = const { Cell::new(Stat { ns: 0, allocs: 0, bytes: 0 }) };
    static ENTER: Cell<(Option<Duration>, u64, u64)> = const { Cell::new((None, 0, 0)) };
}

fn inline_hook(enter: bool) {
    if !HOOKED.load(Relaxed) {
        return;
    }
    if enter {
        ENTER.set((Some(cpu_now()), ALLOCS.load(Relaxed), BYTES.load(Relaxed)));
        return;
    }
    let (t, a, b) = ENTER.get();
    let Some(t) = t else { return };
    let ns = u64::try_from(cpu_now().saturating_sub(t).as_nanos()).unwrap_or(u64::MAX);
    let mut s = INLINE.get();
    s.ns += ns;
    s.allocs += ALLOCS.load(Relaxed) - a;
    s.bytes += BYTES.load(Relaxed) - b;
    INLINE.set(s);
}

fn counters() -> (u64, u64) {
    (ALLOCS.load(Relaxed), BYTES.load(Relaxed))
}

/// Runs `f` once and returns its time and allocations.
fn once<T>(f: impl FnOnce() -> T) -> (T, Stat) {
    let (a, b) = counters();
    let t = cpu_now();
    let out = f();
    let ns = u64::try_from(cpu_now().saturating_sub(t).as_nanos()).unwrap_or(u64::MAX);
    let (a2, b2) = counters();
    (
        out,
        Stat {
            ns,
            allocs: a2 - a,
            bytes: b2 - b,
        },
    )
}

/// One round: repeats `f` for at least `ms` and returns microseconds per call.
/// CPU time of this thread: other processes on a loaded machine do not count.
fn cpu_now() -> Duration {
    let mut t = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `t` is a valid, writable timespec for the call.
    unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &raw mut t) };
    Duration::new(
        u64::try_from(t.tv_sec).unwrap_or(0),
        u32::try_from(t.tv_nsec).unwrap_or(0),
    )
}

/// One round: repeats `f` for at least `ms` of CPU time and returns CPU microseconds per call.
fn round(f: &dyn Fn() -> usize, ms: u64) -> f64 {
    let t0 = cpu_now();
    let mut n = 0u32;
    while cpu_now().saturating_sub(t0) < Duration::from_millis(ms) {
        black_box(f());
        n += 1;
    }
    cpu_now().saturating_sub(t0).as_secs_f64() * 1e6 / f64::from(n)
}

fn median(v: &mut [f64]) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

struct Summary {
    median: f64,
    lo: f64,
    hi: f64,
}

fn summary(mut v: Vec<f64>) -> Summary {
    let m = median(&mut v);
    Summary {
        median: m,
        lo: v[0],
        hi: v[v.len() - 1],
    }
}

fn stages(src: &str, opts: &carve2::Options, reps: u32) -> [Stat; 5] {
    let mut acc = [Stat::default(); 5];
    for _ in 0..reps {
        INLINE.set(Stat::default());
        let (s, sb) = once(|| {
            let s = carve2::source::Source::new(src);
            let out = carve2::block::scan(&s);
            (s, out)
        });
        let (source, out) = s;
        let (mut doc, st) = once(|| carve2::tree::build(&source, out.events, &out.own_starts));
        let inl = INLINE.get();
        let tree = Stat {
            ns: st.ns - inl.ns,
            allocs: st.allocs - inl.allocs,
            bytes: st.bytes - inl.bytes,
        };
        let ((), sr) = once(|| carve2::resolve::resolve(&mut doc, opts));
        let (html, sh) = once(|| carve2::html::render_with_losses_source(&doc, opts, src));
        black_box(html);
        for (a, s) in acc.iter_mut().zip([sb, inl, tree, sr, sh]) {
            a.ns += s.ns;
            a.allocs += s.allocs;
            a.bytes += s.bytes;
        }
    }
    acc.map(|s| Stat {
        ns: s.ns / u64::from(reps),
        allocs: s.allocs / u64::from(reps),
        bytes: s.bytes / u64::from(reps),
    })
}

/// For a sampling profiler: parse plus HTML of one document, again and again.
fn run_loop(secs: u64, path: &str) {
    let src = std::fs::read_to_string(path).unwrap_or_default();
    let opts = carve2::Options::default();
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_secs(secs) {
        black_box(carve2::render(&src, &opts));
    }
}

/// Times the four measures alternately and prints one row per measure.
fn time_document(name: &str, src: &str, opts: &carve2::Options, rounds: u64, ms: u64) {
    let p2 = || {
        black_box(carve2::parse(src));
        1_usize
    };
    let h2 = || carve2::render(src, opts).len();
    let p1 = || {
        black_box(carve::parse(src));
        1_usize
    };
    let h1 = || carve::render_html(&carve::parse(src)).map_or(0, |h| h.len());
    let fs = [&p2 as &dyn Fn() -> usize, &h2, &p1, &h1];
    for f in fs {
        for _ in 0..5 {
            black_box(f());
        }
    }
    // Order per round: 0.2 parse, 0.1 parse, 0.2 HTML, 0.1 HTML.
    let mut times = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
    for _ in 0..rounds {
        for k in [0, 2, 1, 3] {
            times[k].push(round(fs[k], ms));
        }
    }
    let ratio = |x: &[f64], y: &[f64]| summary(x.iter().zip(y).map(|(a, b)| b / a).collect());
    let rows = [
        ("parse", 0, 2, ratio(&times[0], &times[2])),
        ("parse+html", 1, 3, ratio(&times[1], &times[3])),
    ];
    let sums = times.map(summary);
    for (measure, new, old, r) in rows {
        let (x, y) = (&sums[new], &sums[old]);
        println!(
            "{name}\t{}\t{measure}\t{:.0}\t{:.0}\t{:.0}\t{:.0}\t{:.0}\t{:.0}\t{:.2}\t{:.2}\t{:.2}\t{:.2}",
            src.len(),
            x.median,
            x.lo,
            x.hi,
            y.median,
            y.lo,
            y.hi,
            y.median / x.median,
            r.median,
            r.lo,
            r.hi
        );
    }
}

fn print_stages(name: &str, st: &[Stat; 5], v01: [Stat; 2]) {
    let labels = ["block", "inline", "tree", "resolve", "html"];
    for (l, s) in labels.iter().zip(st) {
        println!(
            "{name}\t{l}\t{:.0}\t{}\t{}",
            s.ns as f64 / 1e3,
            s.allocs,
            s.bytes
        );
    }
    println!("{name}\tv01-parse\t-\t{}\t{}", v01[0].allocs, v01[0].bytes);
    println!(
        "{name}\tv01-parse+html\t-\t{}\t{}",
        v01[1].allocs, v01[1].bytes
    );
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut rounds = 15;
    let mut ms = 200;
    while let Some(i) = args.iter().position(|a| a == "--rounds" || a == "--ms") {
        let v: u64 = args.get(i + 1).and_then(|v| v.parse().ok()).unwrap_or(0);
        if args[i] == "--rounds" {
            rounds = v;
        } else {
            ms = v;
        }
        args.drain(i..=i + 1);
    }
    if let Some(i) = args.iter().position(|a| a == "--loop") {
        let secs = args.get(i + 1).and_then(|v| v.parse().ok()).unwrap_or(10);
        run_loop(secs, args.get(i + 2).map_or("", String::as_str));
        return;
    }
    let _ = carve2::profile::INLINE_HOOK.set(inline_hook);
    let opts = carve2::Options::default();
    println!(
        "doc\tbytes\tmeasure\tv02_us\tv02_lo\tv02_hi\tv01_us\tv01_lo\tv01_hi\tspeedup\tround_ratio\tratio_lo\tratio_hi"
    );
    let mut stage_rows = Vec::new();
    for path in &args {
        let src = std::fs::read_to_string(path).expect("readable document");
        let name = path
            .rsplit('/')
            .next()
            .unwrap_or(path)
            .trim_end_matches(".crv")
            .to_string();
        time_document(&name, &src, &opts, rounds, ms);
        let reps = u32::try_from((2_000_000 / src.len().max(1)).clamp(3, 200)).unwrap_or(3);
        HOOKED.store(true, Relaxed);
        let st = stages(&src, &opts, reps);
        HOOKED.store(false, Relaxed);
        let (_, a1) = once(|| carve::parse(&src));
        let (_, a2) = once(|| carve::render_html(&carve::parse(&src)));
        stage_rows.push((name, st, [a1, a2]));
    }
    println!("\ndoc\tstage\tus\tallocs\tbytes");
    for (name, st, v01) in &stage_rows {
        print_stages(name, st, *v01);
    }
}
