#![allow(long_running_const_eval)]

use crate::types::Bitboard;

pub mod types;
mod libs;
pub mod board;

fn main() {
    let mut bb = Bitboard(u64::MAX);
    println!("{}", bb.0);
    bb <<= 1;
    println!("{}", bb.0);
}
