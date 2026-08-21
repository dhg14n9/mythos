use std::ops::{Add, AddAssign};
use crate::nnue::HL;
use crate::types::{Color, Piece, Square};

#[repr(C, align(64))]
#[derive(Copy, Clone)]
pub struct Accumulator([i16; HL]);

impl Accumulator {
    pub fn empty() -> Self {
        Self([0; HL])
    }

    pub fn get(&self, index: usize) -> i16 {
        debug_assert!(index < HL);

        self.0[index]
    }

    pub fn set(&mut self, index: usize, x: i16) {
        self.0[index] = x
    }
}

impl Add for Accumulator {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        let mut output = Self::empty();
        for i in 0..HL {
            output.0[i] = self.0[i] + rhs.0[i]
        }
        output
    }
}

impl AddAssign for Accumulator {
    fn add_assign(&mut self, rhs: Self) {
        for i in 0..HL {
            self.0[i] += rhs.0[i]
        }
    }
}

pub fn feature_index(perspective: Color, piece: Piece, square: Square) -> usize {
    (piece.piece_type() as usize) * 64 + (square.relative_to(perspective) as usize) + if perspective == piece.color() { 0 } else { 384 }
}
