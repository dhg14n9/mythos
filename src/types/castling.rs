use crate::types::Square;
use std::ops::{Index, IndexMut};

#[derive(Copy, Clone)]
pub enum CastlingKind {
    WhiteKing = 0b0001,
    WhiteQueen = 0b0010,
    BlackKing = 0b0100,
    BlackQueen = 0b1000,
}

impl CastlingKind {
    pub const KINDS: [[Self; 2]; 2] = [
        [Self::WhiteKing, Self::WhiteQueen],
        [Self::BlackKing, Self::BlackQueen],
    ];

    pub fn king_landing_square(castling_kind: CastlingKind) -> Square {
        match castling_kind {
            CastlingKind::WhiteKing => Square::G1,
            CastlingKind::WhiteQueen => Square::C1,
            CastlingKind::BlackKing => Square::G8,
            CastlingKind::BlackQueen => Square::C8,
        }
    }
    pub fn rook_landing_square(castling_kind: CastlingKind) -> Square {
        match castling_kind {
            CastlingKind::WhiteKing => Square::F1,
            CastlingKind::WhiteQueen => Square::D1,
            CastlingKind::BlackKing => Square::F8,
            CastlingKind::BlackQueen => Square::D8,
        }
    }
}

impl<T> Index<CastlingKind> for [T] {
    type Output = T;

    fn index(&self, index: CastlingKind) -> &Self::Output {
        &self[index as usize]
    }
}

impl<T> IndexMut<CastlingKind> for [T] {
    fn index_mut(&mut self, index: CastlingKind) -> &mut Self::Output {
        &mut self[index as usize]
    }
}

#[derive(Copy, Clone, Default)]
pub struct Castling(u8);

impl Castling {
    pub const ALL: Self = Castling(0b1111);
    pub const NUM: usize = 16;

    pub const SQUARE_MASK: [u8; Square::NUM] = {
        let mut mask = [0b1111u8; Square::NUM];
        mask[Square::A1 as usize] = 0b1111 & !(CastlingKind::WhiteQueen as u8);
        mask[Square::H1 as usize] = 0b1111 & !(CastlingKind::WhiteKing as u8);
        mask[Square::E1 as usize] =
            0b1111 & !(CastlingKind::WhiteKing as u8 | CastlingKind::WhiteQueen as u8);
        mask[Square::A8 as usize] = 0b1111 & !(CastlingKind::BlackQueen as u8);
        mask[Square::H8 as usize] = 0b1111 & !(CastlingKind::BlackKing as u8);
        mask[Square::E8 as usize] =
            0b1111 & !(CastlingKind::BlackKing as u8 | CastlingKind::BlackQueen as u8);
        mask
    };

    pub fn new(value: u8) -> Self {
        debug_assert!(value < 16);
        Self(value)
    }

    pub fn raw(self) -> usize {
        self.0 as usize
    }

    pub fn is_allowed(self, castling_kind: CastlingKind) -> bool {
        (self.0 & castling_kind as u8) != 0
    }

    pub fn insert(&mut self, castling_kind: CastlingKind) {
        self.0 |= castling_kind as u8
    }

    pub fn remove(&mut self, castling_kind: CastlingKind) {
        self.0 &= !(castling_kind as u8)
    }

    pub fn string(self) -> String {
        if self.0 == 0 {
            return "-".to_string();
        }

        let mut output = String::new();

        for color in 0..2 {
            for kind in CastlingKind::KINDS[color] {
                output += match kind {
                    CastlingKind::WhiteKing => "W",
                    CastlingKind::WhiteQueen => "B",
                    CastlingKind::BlackKing => "w",
                    CastlingKind::BlackQueen => "b",
                }
            }
        }

        output
    }
}
