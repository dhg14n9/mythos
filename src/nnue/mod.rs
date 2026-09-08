use crate::board::board::Board;
use crate::nnue::accumulator::{AccState};
use crate::nnue::network::{Network, evaluate, materialize};
use crate::tables::MAX_PLY;

pub mod accumulator;
pub mod network;
pub mod stats;

pub(crate) const BUCKET_SIZE: usize = 768;
const HL: usize = 512;
pub(crate) const QA: i16 = 255;
const QB: i16 = 64;
const SCALE: i32 = 400;
pub(crate) const OUTPUT_BUCKETS: usize = 8;

#[rustfmt::skip]
const KING_LAYOUT: [usize; 32] = [
    0, 1, 2, 3,
    4, 4, 5, 5,
    6, 6, 6, 6,
    7, 7, 7, 7,
    8, 8, 8, 8,
    8, 8, 8, 8,
    9, 9, 9, 9,
    9, 9, 9, 9,
];

const fn bucket_count(layout: [usize; 32]) -> usize {
    let mut i = 0;
    let mut max = 0;
    while i < 32 {
       if layout[i] > max {
           max = layout[i];
       }
        i += 1;
    }
    max + 1
}

pub(crate) const BUCKET_COUNT: usize = bucket_count(KING_LAYOUT);

const _: () = assert!(BUCKET_COUNT == 10);

pub(crate) const INPUT: usize = BUCKET_SIZE * BUCKET_COUNT;

pub static NETWORK: Network = unsafe { std::mem::transmute(*include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/nets/net.nnue")))};

pub fn eval(board: &Board, accumulator_stack: &mut [AccState; MAX_PLY], ply: usize) -> i32 {
    let us = board.stm();
    materialize(&NETWORK, accumulator_stack, ply, us);
    materialize(&NETWORK, accumulator_stack, ply, !us);

    const DIVISOR: usize = 32usize.div_ceil(OUTPUT_BUCKETS);
    let o_bucket = (board.occ().pop_count() - 2) / DIVISOR;

    evaluate(&NETWORK, &accumulator_stack[ply].accs[us], &accumulator_stack[ply].accs[!us], o_bucket)
}
