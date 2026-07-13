#![allow(long_running_const_eval)]

pub mod bench;
pub mod board;
pub mod movepicker;
pub mod types;
pub mod uci;

mod eval;
mod search;
#[cfg(test)]
mod tests;
