#[cfg(all(target_os = "linux", target_pointer_width = "64"))]
use std::hint::black_box;

#[cfg(all(target_os = "linux", target_pointer_width = "64"))]
#[repr(C)]
struct Timespec {
    seconds: std::ffi::c_long,
    nanoseconds: std::ffi::c_long,
}

#[cfg(all(target_os = "linux", target_pointer_width = "64"))]
extern "C" {
    fn clock_gettime(clock: i32, time: *mut Timespec) -> i32;
}

#[cfg(all(target_os = "linux", target_pointer_width = "64"))]
fn thread_cpu_ns() -> i128 {
    let mut time = Timespec {
        seconds: 0,
        nanoseconds: 0,
    };
    assert_eq!(unsafe { clock_gettime(3, &mut time) }, 0);
    time.seconds as i128 * 1_000_000_000 + time.nanoseconds as i128
}

#[cfg(all(target_os = "linux", target_pointer_width = "64"))]
fn run(source: &str, route: &str, candidate: bool) -> (usize, bool) {
    let (doc, used) = if candidate {
        carve::parse_experimental_event_blocks(source)
    } else {
        (carve::parse(source), false)
    };
    let length = match route {
        "ast" => doc.children.len(),
        "html" => carve::render_html(&doc).unwrap().len(),
        _ => panic!("unknown route"),
    };
    (black_box(length), used)
}

#[cfg(all(target_os = "linux", target_pointer_width = "64"))]
fn main() {
    let args: Vec<String> = std::env::args().collect();
    assert_eq!(
        args.len(),
        5,
        "usage: event_route_cpu baseline|candidate ast|html path reps"
    );
    let candidate = args[1] == "candidate";
    let route = &args[2];
    let source = std::fs::read_to_string(&args[3]).unwrap();
    let reps: usize = args[4].parse().unwrap();
    for _ in 0..20 {
        black_box(run(&source, route, candidate));
    }
    let start = thread_cpu_ns();
    let mut checksum = 0usize;
    let mut used = false;
    for _ in 0..reps {
        let (length, admitted) = run(&source, route, candidate);
        checksum = checksum.wrapping_add(length);
        used |= admitted;
    }
    let ns_per_op = (thread_cpu_ns() - start) as f64 / reps as f64;
    println!(
        "{}\t{}\t{:.1}\t{}\t{}",
        args[1], route, ns_per_op, checksum, used
    );
}

#[cfg(not(all(target_os = "linux", target_pointer_width = "64")))]
fn main() {
    eprintln!("event_route_cpu needs 64-bit Linux thread CPU timing");
}
