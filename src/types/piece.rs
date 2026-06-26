use std::fmt::{Display, Formatter};
use std::ops::{Index, IndexMut};
use crate::types::Color;

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Default)]
#[repr(u8)]
pub enum PieceType {
    Pawn = 0,
    Knight,
    Bishop,
    Rook,
    Queen,
    King,
    #[default] None
}

impl PieceType {
    pub const NUM: usize = 6;
    pub fn new(value: u8) -> Self {
        debug_assert!(value < Self::NUM as u8);

        unsafe { std::mem::transmute(value) }
    }

    pub fn value(self) -> i32 {
        match self {
            PieceType::Pawn => 100,
            PieceType::Knight => 300,
            PieceType::Bishop => 300,
            PieceType::Rook => 500,
            PieceType::Queen => 900,
            PieceType::King => 0,
            PieceType::None => 0
        }
    }

    pub fn parse(ch: char) -> Result<Self, &'static str> {
        match ch.to_ascii_lowercase() {
            'p' => Ok(Self::Pawn),
            'n' => Ok(Self::Knight),
            'b' => Ok(Self::Bishop),
            'r' => Ok(Self::Rook),
            'q' => Ok(Self::Queen),
            'k' => Ok(Self::King),
            _ => Err("Invalid PieceType!")
        }
    }

    pub fn to_char(self) -> char {
        match self {
            PieceType::Pawn => 'p',
            PieceType::Knight => 'n',
            PieceType::Bishop => 'b',
            PieceType::Rook => 'r',
            PieceType::Queen => 'q',
            PieceType::King => 'k',
            PieceType::None => '.'
        }
    }
}

impl Display for PieceType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_char())
    }
}

impl<T> Index<PieceType> for [T] {
    type Output = T;

    fn index(&self, index: PieceType) -> &Self::Output {
        &self [index as usize]
    }
}

impl<T> IndexMut<PieceType> for [T] {
    fn index_mut(&mut self, index: PieceType) -> &mut Self::Output {
        &mut self [index as usize]
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Default)]
#[repr(u8)]
pub enum Piece {
    WhitePawn = 0,
    BlackPawn,
    WhiteKnight,
    BlackKnight,
    WhiteBishop,
    BlackBishop,
    WhiteRook,
    BlackRook,
    WhiteQueen,
    BlackQueen,
    WhiteKing,
    BlackKing,
    #[default] None
}

impl Piece {
    pub const NUM: usize = 12;

    pub fn new(color: Color, piece_type: PieceType) -> Self {
        debug_assert!(piece_type != PieceType::None);

        unsafe { std::mem::transmute((piece_type as u8) << 1 | color as u8) }
    }

    pub fn from_value(value: u8) -> Self {
        debug_assert!(value < Self::NUM as u8);

        unsafe { std::mem::transmute(value) }
    }

    pub fn color(self) -> Color {
        unsafe { std::mem::transmute(self as u8 & 1) }
    }
    pub fn piece_type(self) -> PieceType {
        unsafe { std::mem::transmute(self as u8 >> 1) }
    }
    pub fn value(self) -> i32 {
        self.piece_type().value()
    }

    pub fn parse(ch: char) -> Result<Self, &'static str> {
        let color = if ch.is_ascii_uppercase() { Color::White } else { Color::Black };
        let piece_type = PieceType::parse(ch)?;

        Ok(Self::new(color, piece_type))
    }

    pub fn to_char(self) -> char {
        if self == Piece::None {
            return '.';
        }

        let ch = self.piece_type().to_char();
        match self.color() {
            Color::White => ch.to_ascii_uppercase(),
            Color::Black => ch
        }
    }
}

impl Display for Piece {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_char())
    }
}

impl<T> Index<Piece> for [T] {
    type Output = T;

    fn index(&self, index: Piece) -> &Self::Output {
        &self [index as usize]
    }
}

impl<T> IndexMut<Piece> for [T] {
    fn index_mut(&mut self, index: Piece) -> &mut Self::Output {
        &mut self [index as usize]
    }
}
