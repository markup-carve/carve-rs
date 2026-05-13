//! `carve` CLI — reads Carve source from a file or stdin, writes HTML to stdout.

use std::io::{self, Read, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let source = match args.get(1).map(String::as_str) {
        None | Some("-") => {
            let mut buf = String::new();
            if let Err(err) = io::stdin().read_to_string(&mut buf) {
                eprintln!("carve: cannot read stdin: {err}");
                return ExitCode::FAILURE;
            }
            buf
        }
        Some("-h") | Some("--help") => {
            print_usage();
            return ExitCode::SUCCESS;
        }
        Some(path) => match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(err) => {
                eprintln!("carve: cannot read {path}: {err}");
                return ExitCode::FAILURE;
            }
        },
    };
    let html = carve::to_html(&source);
    let mut stdout = io::stdout().lock();
    if let Err(err) = stdout.write_all(html.as_bytes()) {
        eprintln!("carve: cannot write stdout: {err}");
        return ExitCode::FAILURE;
    }
    if !html.ends_with('\n') {
        let _ = stdout.write_all(b"\n");
    }
    ExitCode::SUCCESS
}

fn print_usage() {
    println!(
        "carve — render Carve markup to HTML\n\n\
         Usage:\n  \
         carve [file]      render file (or stdin when omitted or `-`)\n  \
         carve -h          show this help\n\n\
         Spec: https://markup-carve.github.io/carve/"
    );
}
