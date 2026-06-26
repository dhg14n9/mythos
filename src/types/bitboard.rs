use std::ops::{
    BitAnd, BitAndAssign, BitOr, BitOrAssign, BitXor, BitXorAssign, Not, Shl, ShlAssign, Shr,
    ShrAssign
};
use crate::types::{Direction, File, Rank, Square};

#[derive(Copy, Clone, Eq, PartialEq, Default)]
#[repr(transparent)]
pub struct Bitboard(pub u64);


impl Bitboard {
    pub const FULL: Self = Self(u64::MAX);
    pub const EMPTY: Self = Self(0);
    pub const PAWN_START: [Self; 2] = [Self(0xff00), Self(0xff000000000000)]; // pawn starting row
    pub const FIRST_ROWS: [Self; 2] = [Self(0xff), Self(0xff00000000000000)]; // piece starting row
    pub const THIRD_ROWS: [Self; 2] = [Self(0xff0000), Self(0xff0000000000)]; // row next to pawn
    pub const EN_PASSANT_ROWS: [Self; 2] = [Self(0xff00000000), Self(0xff000000)]; // row a pawn needs to be on when able to take en passant

    pub fn new(value: u64) -> Self {
        Self(value)
    }

    pub fn from_file(file: File) -> Self {
        Self(0x101010101010101u64 << (file as usize))
    }
    pub fn from_rank(rank: Rank) -> Self {
        Self(0xffu64 << (rank as usize) * 8)
    }
    pub fn from_square(square: Square) -> Self {
        Self(1 << (square as usize))
    }

    pub fn set(&mut self, square: Square) {
        self.0 |= 1 << (square as usize)
    }
    pub fn clear(&mut self, square: Square) {
        self.0 &= !(1 << (square as usize))
    }
    pub fn offset(&mut self, value: i8) {
        if value > 0 { self.0 <<= value } else { self.0 >>= -value }
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
    pub fn is_multiple(self) -> bool {
        self.0 != 0 && self.0 & (self.0 - 1) != 0
    }
    pub fn contains(self, square: Square) -> bool {
        (self.0 & (1u64 << square as usize)) != 0
    }

    pub fn lsb(self) -> Square {
        if self.is_empty() {
            return Square::None;
        }
        Square::new(self.0.trailing_zeros() as u8)
    }
    pub fn msb(self) -> Square {
        if self.is_empty() {
            return Square::None
        }
        Square::new((63 - self.0.leading_zeros()) as u8)
    }

    // mask for assisted shifting to avoid bit-jumping
    const SHIFT_MASK: [Self; 8] = [
        Self(0xffffffffffffff),
        Self(0xffffffffffffff00),
        Self(0xfefefefefefefefe),
        Self(0x7f7f7f7f7f7f7f7f),
        Self(0xfefefefefefefe),
        Self(0x7f7f7f7f7f7f7f),
        Self(0xfefefefefefefe00),
        Self(0x7f7f7f7f7f7f7f00)
    ];
    const SHIFT_NUMBER: [i8; 8] = [
        8, -8, -1, 1, 7, 9, -9, -7
    ];

    // assisted shifting
    pub fn shift(&mut self, direction: Direction) {
        *self &= Self::SHIFT_MASK[direction];
        self.offset(Self::SHIFT_NUMBER[direction])
    }
}

impl BitAnd for Bitboard {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self::Output {
        Self(self.0 & rhs.0)
    }
}

impl BitAndAssign for Bitboard {
    fn bitand_assign(&mut self, rhs: Self) {
        self.0 &= rhs.0;
    }
}

impl BitOr for Bitboard {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for Bitboard {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl BitXor for Bitboard {
    type Output = Self;
    fn bitxor(self, rhs: Self) -> Self::Output {
        Self(self.0 ^ rhs.0)
    }
}

impl BitXorAssign for Bitboard {
    fn bitxor_assign(&mut self, rhs: Self) {
        self.0 ^= rhs.0;
    }
}

impl Not for Bitboard {
    type Output = Self;
    fn not(self) -> Self::Output {
        Self(!self.0)
    }
}

impl Shl<u32> for Bitboard {
    type Output = Self;
    fn shl(self, rhs: u32) -> Self::Output {
        Self(self.0 << rhs)
    }
}

impl ShlAssign<u32> for Bitboard {
    fn shl_assign(&mut self, rhs: u32) {
        self.0 <<= rhs;
    }
}

impl Shr<u32> for Bitboard {
    type Output = Self;
    fn shr(self, rhs: u32) -> Self::Output {
        Self(self.0 >> rhs)
    }
}

impl ShrAssign<u32> for Bitboard {
    fn shr_assign(&mut self, rhs: u32) {
        self.0 >>= rhs;
    }
}