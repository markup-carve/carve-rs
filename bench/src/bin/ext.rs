//! Times extension-hosted render against core render, 0.2 and 0.1 (carve-rs#4366).
//!
//! `cargo run --release --manifest-path bench/Cargo.toml --bin ext -- [--rounds N] [--only BUNDLE] [--profile BUNDLE]`
//!
//! Per size (2, 8, 32, 128 KiB) and bundle: the median thread CPU time, in alternating rounds, of `Carve::render`
//! with the bundle registered, core `render`, and 0.1.14 `render_html_with_options`
//! with the matching 0.1 extension, and the factor hosted / core.
#![allow(clippy::cast_precision_loss)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::time::{Duration, Instant};

struct Counting;

static ALLOCS: AtomicU64 = AtomicU64::new(0);

// SAFETY: forwards every call to the system allocator unchanged; the counter is a side effect only.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Relaxed);
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        ALLOCS.fetch_add(1, Relaxed);
        unsafe { System.realloc(p, l, n) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

/// Allocations of one call of `f`, which a loaded machine does not disturb.
fn allocs(f: &dyn Fn() -> usize) -> u64 {
    let before = ALLOCS.load(Relaxed);
    black_box(f());
    ALLOCS.load(Relaxed) - before
}

/// User-space instructions retired by the calling thread, which neither the
/// load nor the clock of the machine moves; `None` where the kernel refuses
/// the counter.
struct Instructions(i32);

impl Instructions {
    fn open() -> Option<Self> {
        // perf_event_attr: type HARDWARE, size, config INSTRUCTIONS, flags
        // disabled | exclude_kernel | exclude_hv.
        let mut attr = [0u64; 16];
        attr[0] = 128 << 32;
        attr[1] = 1;
        attr[5] = 1 | (1 << 5) | (1 << 6);
        // SAFETY: `attr` is a zeroed perf_event_attr of the size it declares.
        let fd = unsafe { libc::syscall(libc::SYS_perf_event_open, attr.as_ptr(), 0, -1, -1, 0) };
        i32::try_from(fd).ok().filter(|fd| *fd >= 0).map(Self)
    }

    fn count(&self, f: &dyn Fn() -> usize) -> u64 {
        let mut n = 0u64;
        // SAFETY: reset, enable and disable on an owned counter fd; the read
        // fills one u64.
        unsafe {
            libc::ioctl(self.0, 0x2403, 0);
            libc::ioctl(self.0, 0x2400, 0);
            black_box(f());
            libc::ioctl(self.0, 0x2401, 0);
            libc::read(self.0, (&raw mut n).cast(), 8);
        }
        n
    }

    /// The least count of a few calls, after a warm-up call.
    fn least(&self, f: &dyn Fn() -> usize) -> u64 {
        black_box(f());
        (0..5).map(|_| self.count(f)).min().unwrap_or(0)
    }
}

use carve::api::{self, Carve, Options, extensions};

const UNIT: &str = "## Section heading\n\nA paragraph with *emphasis*, `code`, a [link](https://example.com) and \"quotes\" in it.\n\n```mermaid\ngraph TD; A-->B\n```\n\n- first item\n- second item\n\n```math\na^2 + b^2 = c^2\n```\n\n::: code-group\n```js [NPM]\nnpm i\n```\n\n```sh\nyarn add\n```\n:::\n\n```js\nlet a = 1 <1>\n```\n<1> First\n\n";

fn doc(size: usize) -> String {
    let mut s = String::with_capacity(size + UNIT.len());
    while s.len() < size {
        s.push_str(UNIT);
    }
    s
}

/// CPU time of the calling thread, which a loaded machine disturbs less than
/// wall time.
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

/// Median thread CPU times of `fs`, measured in alternating rounds so a
/// change of the machine's load reaches every function alike.
fn medians<const N: usize>(rounds: usize, fs: [&dyn Fn() -> usize; N]) -> [f64; N] {
    for f in &fs {
        for _ in 0..3 {
            black_box(f());
        }
    }
    let mut times: [Vec<f64>; N] = std::array::from_fn(|_| Vec::with_capacity(rounds));
    for _ in 0..rounds {
        for (f, t) in fs.iter().zip(times.iter_mut()) {
            let s = cpu_now();
            black_box(f());
            t.push((cpu_now() - s).as_secs_f64() * 1e3);
        }
    }
    times.map(|mut t| {
        t.sort_by(f64::total_cmp);
        t[t.len() / 2]
    })
}

fn host(name: &str) -> Carve {
    let mut c = Carve::new();
    let mut add = |b: extensions::Bundle| c.register(b).unwrap();
    let mermaid = || {
        extensions::fenced_render(extensions::FencedRenderOptions {
            language: vec!["mermaid".into()],
            ..Default::default()
        })
        .unwrap()
    };
    match name {
        "details" => add(extensions::details()),
        "mermaid" => add(mermaid()),
        "math" => add(extensions::math_block(
            extensions::MathBlockOptions::default(),
        )),
        "codeGroup" => add(extensions::code_group(
            extensions::CodeGroupOptions::default(),
        )),
        "callouts" => add(extensions::code_callouts()),
        "fenced4" => {
            add(mermaid());
            add(extensions::math_block(
                extensions::MathBlockOptions::default(),
            ));
            add(extensions::code_group(
                extensions::CodeGroupOptions::default(),
            ));
            add(extensions::code_callouts());
        }
        _ => unreachable!(),
    }
    c
}

fn old_render(src: &str, name: &str) -> usize {
    let details = carve_0_1::Details::new();
    let mermaid = carve_0_1::FencedRender::mermaid();
    let math = carve_0_1::MathBlock::new();
    let group = carve_0_1::CodeGroup::new();
    let callouts = carve_0_1::CodeCallouts::new();
    let mut o = carve_0_1::Options::new();
    o = match name {
        "details" => o.with_extension(&details),
        "mermaid" => o.with_extension(&mermaid),
        "math" => o.with_extension(&math),
        "codeGroup" => o.with_extension(&group),
        "callouts" => o.with_extension(&callouts),
        _ => o
            .with_extension(&mermaid)
            .with_extension(&math)
            .with_extension(&group)
            .with_extension(&callouts),
    };
    let d = carve_0_1::parse_with_options(src, &o);
    carve_0_1::render_html_with_options(&d, &o).map_or(0, |h| h.len())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let rounds = args
        .iter()
        .position(|a| a == "--rounds")
        .and_then(|i| args.get(i + 1)?.parse().ok())
        .unwrap_or(30);
    let opts = Options::default();
    // `--dump DIR`: writes the hosted render of every spec input under each
    // standard extension to DIR, one file per extension, to diff two builds.
    if let Some(dir) = args
        .iter()
        .position(|a| a == "--dump")
        .and_then(|i| args.get(i + 1))
    {
        dump(std::path::Path::new(dir));
        return;
    }
    // `--profile NAME`: renders 128 KiB with the bundle NAME for ten seconds, for a profiler.
    if let Some(name) = args
        .iter()
        .position(|a| a == "--profile")
        .and_then(|i| args.get(i + 1))
    {
        let carve = host(name);
        let src = doc(128 * 1024);
        if std::env::var_os("EXT_T").is_some() {
            for _ in 0..5 {
                black_box(carve.render(src.as_str(), &opts).map_or(0, |h| h.len()));
            }
            return;
        }
        let start = Instant::now();
        while start.elapsed().as_secs() < 10 {
            black_box(carve.render(src.as_str(), &opts).map_or(0, |h| h.len()));
        }
        return;
    }
    // `--only NAME`: times that bundle alone.
    let only = args
        .iter()
        .position(|a| a == "--only")
        .and_then(|i| args.get(i + 1));
    // `--count`: user-space instructions per render instead of time.
    if args.iter().any(|a| a == "--count") {
        count(only.map(String::as_str), &opts);
        return;
    }
    println!(
        "| bundle | size | core ms | hosted ms | factor | 0.1.14 hosted ms | hosted/0.1 | core allocs | hosted allocs |"
    );
    for name in BUNDLES {
        if only.is_some_and(|o| o != name) {
            continue;
        }
        let carve = host(name);
        for kib in [2usize, 8, 32, 128] {
            let src = doc(kib * 1024);
            let r = if kib >= 128 { rounds.min(15) } else { rounds };
            let [core, hosted, old] = medians(
                r,
                [
                    &|| api::render(src.as_str(), &opts).map_or(0, |h| h.len()),
                    &|| carve.render(src.as_str(), &opts).map_or(0, |h| h.len()),
                    &|| old_render(&src, name),
                ],
            );
            let core_allocs = allocs(&|| api::render(src.as_str(), &opts).map_or(0, |h| h.len()));
            let hosted_allocs =
                allocs(&|| carve.render(src.as_str(), &opts).map_or(0, |h| h.len()));
            println!(
                "| {name} | {kib}k | {core:.3} | {hosted:.3} | {:.2} | {old:.3} | {:.2} | {core_allocs} | {hosted_allocs} |",
                hosted / core,
                hosted / old
            );
        }
    }
}

const BUNDLES: [&str; 6] = [
    "details",
    "mermaid",
    "math",
    "codeGroup",
    "callouts",
    "fenced4",
];

fn count(only: Option<&str>, opts: &Options) {
    let Some(counter) = Instructions::open() else {
        eprintln!("the kernel refuses the instruction counter");
        return;
    };
    println!(
        "| bundle | size | core Minstr | hosted Minstr | factor | 0.1.14 Minstr | hosted/0.1 |"
    );
    for name in BUNDLES {
        if only.is_some_and(|o| o != name) {
            continue;
        }
        let carve = host(name);
        for kib in [2usize, 8, 32, 128] {
            let src = doc(kib * 1024);
            let core = counter.least(&|| api::render(src.as_str(), opts).map_or(0, |h| h.len()));
            let hosted = counter.least(&|| carve.render(src.as_str(), opts).map_or(0, |h| h.len()));
            let old = counter.least(&|| old_render(&src, name));
            let m = |n: u64| n as f64 / 1e6;
            println!(
                "| {name} | {kib}k | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} |",
                m(core),
                m(hosted),
                hosted as f64 / core as f64,
                m(old),
                hosted as f64 / old as f64
            );
        }
    }
}

const STANDARD: &[&str] = &[
    "carve.fencedRender.mermaid",
    "carve.glossary",
    "carve.index",
    "carve.references",
    "carve.mathBlock",
    "carve.codeGroup",
    "carve.codeCallouts",
    "carve.details",
    "carve.spoiler",
    "carve.listTable",
    "carve.colorSwatch",
    "carve.tabNormalize",
    "carve.inlineFootnotesToParentheses",
    "carve.semanticSpan",
    "carve.smartQuotes",
    "carve.externalLinks",
    "carve.headingLevelShift",
    "carve.headingPermalinks",
    "carve.headingNumbers",
    "carve.headingReference",
    "carve.toc",
    "carve.wiki",
    "carve.tabs",
];

fn inputs() -> Vec<String> {
    // One JSON string per line: the spec inputs, which nest past serde_json's
    // recursion limit as whole files.
    let path = std::env::var("EXT_INPUTS").expect("EXT_INPUTS names the inputs file");
    let mut out: Vec<String> = std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let ext: Vec<String> = out[out.len() - 94..].to_vec();
    // Neighbors: each extension case after a heading and before a list, and pairs.
    for (i, a) in ext.iter().enumerate() {
        out.push(format!("# Top\n\n{a}\n- item [^n]\n\n[^n]: note\n"));
        out.push(format!("> {}\n", a.replace('\n', "\n> ")));
        out.push(format!("- {}\n", a.replace('\n', "\n  ")));
        out.push(format!("{a}\n{}", ext[(i + 1) % ext.len()]));
    }
    out.extend(family());
    out.push(doc(8 * 1024));
    out
}

/// Claimed and transformed nodes next to what the resolve fills in: ref
/// links, notes, abbreviations, headings and ids, in each container.
fn family() -> Vec<String> {
    let lines = [
        "<1> See [the ref][r] and [^n].",
        "<1> *Term* HTML and `code`",
        "<1> One\n<2> Two [x](u)",
        "<1> {#cid} Marked",
    ];
    let blocks = [
        "```js\nlet a = 1 <1>\nlet b = 2 <2>\n```\n{LINE}\n",
        "::: code-group\n```js [A]\na <1>\n```\n\n```sh\nb\n```\n:::\n{LINE}\n",
        "::: details \"S [r]\"\n# Inside\n\n{LINE}\n:::\n",
        "```mermaid\ngraph TD; A-->B <1>\n```\n{LINE}\n",
        "```math\nx <1>\n```\n{LINE}\n",
    ];
    let tail = "\n# Head\n\n# Head\n\n[r]: /dest\n[^n]: A note.\n*[HTML]: Hyper\n";
    let mut out = Vec::new();
    for block in blocks {
        for line in lines {
            let body = block.replace("{LINE}", line);
            for wrap in 0..4 {
                let wrapped = match wrap {
                    1 => format!("> {}", body.trim_end().replace('\n', "\n> ")),
                    2 => format!("- {}", body.trim_end().replace('\n', "\n  ")),
                    3 => format!("::: note\n{body}:::\n"),
                    _ => body.clone(),
                };
                out.push(format!("{wrapped}{tail}"));
                out.push(format!("# Top {{#t}}\n\n{wrapped}\n{wrapped}{tail}"));
            }
        }
    }
    out
}

fn dump(dir: &std::path::Path) {
    std::fs::create_dir_all(dir).unwrap();
    let inputs = inputs();
    let opts = Options::default();
    let mut configs: Vec<(String, Carve)> = STANDARD
        .iter()
        .map(|name| {
            let mut c = Carve::new();
            let _ = c.enable_standard(name, true);
            ((*name).to_string(), c)
        })
        .collect();
    let mut all = Carve::new();
    for name in STANDARD {
        let _ = all.enable_standard(name, true);
    }
    configs.push(("all".into(), all));
    configs.push(("fenced4".into(), host("fenced4")));
    for (name, carve) in &configs {
        let mut text = String::new();
        for (i, input) in inputs.iter().enumerate() {
            let rendered = carve.render_with_diagnostics(input.as_str(), &opts);
            let strict = carve.render(input.as_str(), &opts);
            let line = match rendered {
                Ok(r) => serde_json::json!([
                    i,
                    r.html,
                    serde_json::to_value(&r.diagnostics).unwrap(),
                    r.suppressed_diagnostics,
                    strict.map_err(|e| e.to_string())
                ]),
                Err(e) => serde_json::json!([i, e.to_string()]),
            };
            text.push_str(&line.to_string());
            text.push('\n');
        }
        std::fs::write(dir.join(format!("{name}.jsonl")), text).unwrap();
    }
    println!("{} inputs, {} configs", inputs.len(), configs.len());
}
