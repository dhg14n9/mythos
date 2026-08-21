use crate::board::board::Board;
use crate::nnue::accumulator::{feature_index, Accumulator};
use crate::nnue::{HL, INPUT};
use crate::types::Color;

const _: () = assert!(size_of::<Network>() == 394_816);

#[repr(C)]
pub struct Network {
    feature_weights: [Accumulator; INPUT],
    feature_bias: Accumulator,
    output_weights: [i16; 2 * HL],
    output_bias: i16
}

pub fn refresh(net: &Network, board: &Board, perspective: Color) -> Accumulator {
    let mut result = net.feature_bias;
    let occ = board.occ();

    for square in occ {
        let piece = board.piece_at(square);
        let index = feature_index(perspective, piece, square);
        result += net.feature_weights[index]
    }

    result
}
