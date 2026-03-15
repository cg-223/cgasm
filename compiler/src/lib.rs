#![feature(iter_collect_into)]
#![allow(unused)]

use crate::{lexer::Unit, parser::ParseUnit};
pub mod lexer;
pub mod parser;

pub fn run(code: &str, inputs: Option<Vec<i64>>) {
    let start = chrono::Utc::now();

    let unit = Unit::lex_source(code);
    let mut parser = ParseUnit::parse(&unit);
    println!("{:?}", parser.late_instrs());
    if let Some(inputs) = inputs {
        parser.add_inputs(inputs)
    }
    parser.better_execute();
    let since_now = chrono::Utc::now().signed_duration_since(start);
    println!("elapsed: {}", since_now.as_seconds_f64());
}
