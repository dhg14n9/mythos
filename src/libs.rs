#![allow(long_running_const_eval)]

pub mod types;
pub mod board;
pub mod movepicker;
pub mod uci;
pub mod bench;


#[cfg(test)]
mod tests;
mod eval;
