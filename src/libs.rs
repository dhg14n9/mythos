#![allow(long_running_const_eval)]

pub mod bench;
pub mod board;
pub mod movepicker;
pub mod tunables;
pub mod types;
pub mod uci;
mod tables;

mod search;
#[cfg(test)]
mod tests;
pub mod nnue;
