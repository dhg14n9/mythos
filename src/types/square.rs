use std::ops::{Add, BitXor, Div, Index, IndexMut};
use crate::types::Color;

#[derive(Copy, Clone, Eq, PartialEq, PartialOrd, Debug)]
#[repr(u8)]
#[rustfmt::skip]
pub enum File {
    A, B, C, D, E, F, G, H,
}

impl File {
    pub const NUM: usize = 8;
    pub const CASTLE_KING_FILE: [File; 2] = [File::C, File::G];
    pub const CASTLE_ROOK_FILE: [File; 2] = [File::D, File::F];

    pub const ALL: [Self; Self::NUM] = [
        File::A,
        File::B,
        File::C,
        File::D,
        File::E,
        File::F,
        File::G,
        File::H,
    ];
    pub fn new(value: u8) -> Self {
        debug_assert!(value < Self::NUM as u8);

        unsafe { std::mem::transmute(value) }
    }

    pub fn is_kingside(self) -> bool {
        self > Self::D
    }
}

impl<T> Index<File> for [T] {
    type Output = T;

    fn index(&self, index: File) -> &Self::Output {
        &self [index as usize]
    }
}

impl<T> IndexMut<File> for [T] {
    fn index_mut(&mut self, index: File) -> &mut Self::Output {
        &mut self [index as usize]
    }
}

#[derive(Copy, Clone, Eq, PartialEq, PartialOrd, Debug)]
#[repr(u8)]
#[rustfmt::skip]
pub enum Rank {
    R1, R2, R3, R4, R5, R6, R7, R8,
}

impl Rank {
    pub const NUM: usize = 8;
    pub const PROMOTION_RANK: [Self; 2] = [Rank::R8, Rank::R1];
    pub const PRE_PROMOTION_RANK: [Self; 2] = [Rank::R7, Rank::R2];
    pub const PAWN_START_RANK: [Self; 2] = [Rank::R2, Rank::R7];
    pub const ALL: [Self; Self::NUM] = [
        Rank::R1,
        Rank::R2,
        Rank::R3,
        Rank::R4,
        Rank::R5,
        Rank::R6,
        Rank::R7,
        Rank::R8

    ];
    pub fn new(value: u8) -> Self {
        debug_assert!(value < Self::NUM as u8);

        unsafe { std::mem::transmute(value) }
    }
}

impl<T> Index<Rank> for [T] {
    type Output = T;

    fn index(&self, index: Rank) -> &Self::Output {
        &self [index as usize]
    }
}

impl<T> IndexMut<Rank> for [T] {
    fn index_mut(&mut self, index: Rank) -> &mut Self::Output {
        &mut self [index as usize]
    }
}

#[derive(Copy, Clone, Eq, PartialEq, PartialOrd, Debug, Default)]
#[repr(u8)]
#[rustfmt::skip]
pub enum Square {
    A1, B1, C1, D1, E1, F1, G1, H1,
    A2, B2, C2, D2, E2, F2, G2, H2,
    A3, B3, C3, D3, E3, F3, G3, H3,
    A4, B4, C4, D4, E4, F4, G4, H4,
    A5, B5, C5, D5, E5, F5, G5, H5,
    A6, B6, C6, D6, E6, F6, G6, H6,
    A7, B7, C7, D7, E7, F7, G7, H7,
    A8, B8, C8, D8, E8, F8, G8, H8,
    #[default]
    None,
}

impl Square {
    pub const NUM: usize = 64;
    pub fn new(value: u8) -> Self {
        debug_assert!(value < Self::NUM as u8);

        unsafe { std::mem::transmute(value) }
    }

    pub fn from_rank_file(rank: Rank, file: File) -> Self {
        Self::new((file as u8) | ((rank as u8) << 3))
    }

    pub fn rank(self) -> Rank {
        unsafe { std::mem::transmute(self as u8 >> 3) }
    }

    pub fn file(self) -> File {
        unsafe { std::mem::transmute(self as u8 & 7 ) }
    }

    pub fn offset(self, value: i8) -> Self {
        let value = self as i8 + value;
        debug_assert!(value >= 0 && value < Self::NUM as i8);

        Self::new(value as u8)
    }

    pub fn flip_rank(self) -> Self {
        Self::new(self as u8 ^ 56)
    }

    pub fn relative_to(self, color: Color) -> Self {
        match color {
            Color::White => self,
            Color::Black => self.flip_rank()
        }
    }
    
    pub fn is_none(self) -> bool {
        matches!(self, Self::None)
    }

    pub fn is_kingside(self) -> bool {
        self.file().is_kingside()
    }

    pub fn parse(value: &str) -> Result<Self, &'static str> {
        // algebraic notation to Square

            match value.as_bytes() {
                [file @ b'a'..b'h', rank @ b'1'..b'8'] => {
                    let rank = rank - b'1';
                    let file = file - b'a';
                    Ok(Self::new(file + (rank << 3)))
                },
                _ => Err("Invalid Square")
            }
    }
}


impl BitXor<u8> for Square {
    type Output = Square;

    fn bitxor(self, rhs: u8) -> Self::Output {
        Square::new(self as u8 ^ rhs)
    }
}

impl Add for Square {
    type Output = Square;

    fn add(self, rhs: Self) -> Self::Output {
        Square::new(self as u8 + rhs as u8)
    }
}

impl Div<u8> for Square {
    type Output = Square;

    fn div(self, rhs: u8) -> Self::Output {
        Square::new(self as u8 / rhs)
    }
}

impl<T> Index<Square> for [T] { 
    type Output = T;
    
    fn index(&self, index: Square) -> &Self::Output {
        &self [index as usize]
    }
}

impl<T> IndexMut<Square> for [T] {
    fn index_mut(&mut self, index: Square) -> &mut Self::Output {
        &mut self [index as usize]
    }
}
