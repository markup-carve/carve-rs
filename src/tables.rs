//! Rule tables generated from `spec-data/*.json` at build time.
#![allow(dead_code)]

include!(concat!(env!("OUT_DIR"), "/tables_gen.rs"));
