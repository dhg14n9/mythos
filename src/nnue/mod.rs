use crate::board::board::Board;
use crate::nnue::network::{refresh, Network, evaluate};

pub mod accumulator;
pub mod network;

const INPUT: usize = 768;
const HL: usize = 256;
const QA: i16 = 255;
const QB: i16 = 64;
const SCALE: i32 = 400;

static NETWORK: Network = unsafe { std::mem::transmute(*include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/nets/net.nnue")))};

pub fn eval(board: &Board) -> i32 {
    let us = board.stm();

    let us_accum = refresh(&NETWORK, board, us);
    let them_accum = refresh(&NETWORK, board, !us);

    evaluate(&NETWORK, &us_accum, &them_accum)
}
