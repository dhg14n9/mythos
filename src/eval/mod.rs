use std::ops::{Add, AddAssign, Mul, Sub, SubAssign};

pub mod eval;
mod piece_square;

// mg, eg
#[derive(Copy, Clone, Default)]
struct S(i32, i32);

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