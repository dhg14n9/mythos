use crate::board::board::Board;
use crate::nnue::accumulator::Accumulator;
use crate::nnue::network::{refresh, Network, evaluate};
use crate::tables::MAX_PLY;

pub mod accumulator;
pub mod network;

const INPUT: usize = 768;
const HL: usize = {
    let bytes = env!("MYTHOS_HL").as_bytes();
    let mut hl = 0;
    let mut i = 0;
    while i < bytes.len() {
        hl = hl * 10 + (bytes[i] - b'0') as usize;
        i += 1;
    }
    hl
};
const QA: i16 = 255;
const QB: i16 = 64;
const SCALE: i32 = 400;

// Path from build.rs too: the EVALFILE= OpenBench passes to make, else nets/net.nnue.
pub static NETWORK: Network = unsafe { std::mem::transmute(*include_bytes!(env!("EVALFILE")))};

pub fn eval(board: &Board, accumulator_stack: &[[Accumulator; 2]; MAX_PLY], ply: usize) -> i32 {
    let us = board.stm();

    evaluate(&NETWORK, &accumulator_stack[ply][us], &accumulator_stack[ply][!us])
}
