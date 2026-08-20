use crate::nnue::accumulator::Accumulator;
use crate::nnue::{HL, INPUT};

const _: () = assert!(size_of::<Network>() == 394_816);

#[repr(C)]
pub struct Network {
    feature_weights: [Accumulator; INPUT],
    feature_bias: Accumulator,
    output_weights: [i16; 2 * HL],
    output_bias: i16
}


