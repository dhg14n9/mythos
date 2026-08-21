use crate::board::board::Board;
use crate::nnue::accumulator::{feature_index, Accumulator};
use crate::nnue::{HL, INPUT, QA, QB, SCALE};
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

pub fn evaluate(net: &Network, us: &Accumulator, them: &Accumulator) -> i32 {
    let mut sum = 0;

    for i in 0..HL {
        sum += screlu(us.get(i)) * net.output_weights[i] as i32;
        sum += screlu(them.get(i)) * net.output_weights[HL + i] as i32;
    }

    sum /= QA as i32;
    sum += net.output_bias as i32;
    sum *= SCALE;
    sum /= (QA * QB) as i32;

    sum
}

fn screlu(x: i16) -> i32 {
    let y = i32::from(x).clamp(0, i32::from(QA));
    y * y
}
