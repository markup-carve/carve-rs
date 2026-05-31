//! `carve` CLI — reads Carve source from a file or stdin, writes HTML to stdout.

use std::io::{self, Read, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut options = carve::Options::new();
    let mut input_path: Option<String> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print_usage();
                return ExitCode::SUCCESS;
            }
            "--mention-url" => {
                let Some(value) = args.next() else {
                    eprintln!("carve: --mention-url requires a template");
                    return ExitCode::FAILURE;
                };
                options = options.with_mention_url(value);
            }
            "--tag-url" => {
                let Some(value) = args.next() else {
                    eprintln!("carve: --tag-url requires a template");
                    return ExitCode::FAILURE;
                };
                options = options.with_tag_url(value);
            }
            "--emoji" => {
                let Some(value) = args.next() else {
                    eprintln!("carve: --emoji requires name=value");
                    return ExitCode::FAILURE;
                };
                let Some((name, glyph)) = value.split_once('=') else {
                    eprintln!("carve: --emoji requires name=value");
                    return ExitCode::FAILURE;
                };
                options = options.with_emoji(name, glyph);
            }
            "-" => input_path = None,
            path if path.starts_with('-') => {
                eprintln!("carve: unknown option: {path}");
                return ExitCode::FAILURE;
            }
            path => {
                if input_path.is_some() {
                    eprintln!("carve: multiple input files specified");
                    return ExitCode::FAILURE;
                }
                input_path = Some(path.to_string());
            }
        }
    }

    let source = match input_path.as_deref() {
        None => {
            let mut buf = String::new();
            if let Err(err) = io::stdin().read_to_string(&mut buf) {
                eprintln!("carve: cannot read stdin: {err}");
                return ExitCode::FAILURE;
            }
            buf
        }
        Some(path) => match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(err) => {
                eprintln!("carve: cannot read {path}: {err}");
                return ExitCode::FAILURE;
            }
        },
    };
    let html = carve::to_html_with_options(&source, &options);
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
         carve [options] [file]      render file (or stdin when omitted or `-`)\n  \
         carve -h                    show this help\n\n\
         Options:\n  \
         --mention-url TEMPLATE      render @mentions as links\n  \
         --tag-url TEMPLATE          render #tags as links\n  \
         --emoji NAME=VALUE          map :NAME: to VALUE (repeatable)\n\n\
         Spec: https://markup-carve.github.io/carve/"
    );
}
