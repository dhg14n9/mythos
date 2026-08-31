use crate::board::board::Board;
use crate::nnue::accumulator::{AccState};
use crate::nnue::network::{Network, evaluate, materialize};
use crate::tables::MAX_PLY;

pub mod accumulator;
pub mod network;

const INPUT: usize = 768;
const HL: usize = 512;
pub(crate) const QA: i16 = 255;
const QB: i16 = 64;
const SCALE: i32 = 400;

pub static NETWORK: Network = unsafe { std::mem::transmute(*include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/nets/net.nnue")))};

pub fn eval(board: &Board, accumulator_stack: &mut [AccState; MAX_PLY], ply: usize) -> i32 {
    let us = board.stm();
    materialize(&NETWORK, accumulator_stack, ply, us);
    materialize(&NETWORK, accumulator_stack, ply, !us);

    evaluate(&NETWORK, &accumulator_stack[ply].accs[us], &accumulator_stack[ply].accs[!us])
}
