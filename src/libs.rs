#![allow(long_running_const_eval)]

pub mod bench;
pub mod board;
pub mod movepicker;
pub mod types;
pub mod uci;
pub mod tables;

pub mod eval;
pub mod search;
pub mod stats;
#[cfg(test)]
mod tests;
