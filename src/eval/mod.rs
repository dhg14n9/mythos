use std::ops::{Add, AddAssign, Mul, Sub, SubAssign};
use crate::types::{Bitboard, Color};

pub mod eval;
mod piece_square;
mod pawn;
mod mobility;

// mg, eg
#[derive(Copy, Clone, Default)]
pub struct S(i32, i32);

impl Add for S {
    type Output = S;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            0: self.0 + rhs.0,
            1: self.1 + rhs.1
        }
    }
}

impl AddAssign for S {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
        self.1 += rhs.1;
    }
}

impl Sub for S {
    type Output = S;

    fn sub(self, rhs: Self) -> Self::Output {
        Self {
            0: self.0 - rhs.0,
            1: self.1 - rhs.1
        }
    }
}

impl SubAssign for S {
    fn sub_assign(&mut self, rhs: Self) {
        self.0 -= rhs.0;
        self.1 -= rhs.1;
    }
}

impl Mul<i32> for S {
    type Output = S;

    fn mul(self, rhs: i32) -> Self::Output {
        Self {
            0: self.0 * rhs,
            1: self.1 * rhs
        }
    }
}

fn s_color(s: S, color: Color) -> S {
    s * match color {
        Color::White => {1}
        Color::Black => {-1}
    }
}

// spread left and right 
fn spread(bb: Bitboard) -> Bitboard {
    ((bb & Bitboard::NOT_FILE_A) >> 1) | ((bb & Bitboard::NOT_FILE_H) << 1)
}