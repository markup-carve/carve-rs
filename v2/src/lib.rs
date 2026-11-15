//! Carve 0.2 engine, stage 1: parse (source to tree), resolve and render HTML,
//! built from the 0.2 specification.
#![forbid(unsafe_code)]

pub mod ast;
pub mod tables;
pub mod attr;
pub mod chars;
pub mod source;
pub mod options;
pub mod html;
pub mod inline;
pub mod resolve;
