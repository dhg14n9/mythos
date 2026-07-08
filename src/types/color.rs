use std::fmt::{Display, Formatter};
use std::ops::{Index, IndexMut, Not};

#[derive(Copy, Clone, Eq, PartialEq)]
#[repr(u8)]
#[rustfmt:: skip]
pub enum Color {
    White = 0,
    Black = 1
}

impl Color {
    pub const NUM: usize = 2;
    pub const ALL: [Color; 2] = [Color::White, Color::Black];

    pub fn new(value: u8) -> Self {
        debug_assert!(value < Self::NUM as u8);

        unsafe { std::mem::transmute(value) }
    }

    pub fn parse(ch: char) -> Result<Self, &'static str> {
        match ch {
            'w' => Ok(Self::White),
            'b' => Ok(Self::Black),
            _ => Err("Invalid Color!")
        }
    }
    
    pub fn char(self) -> char {
        match self {
            Color::White => {'w'}
            Color::Black => {'b'}
        }
    }
}

impl Not for Color {
    type Output = Self;
    fn not(self) -> Self::Output {
        match self {
            Color::White => Color::Black,
            Color::Black => Color::White
        }
    }
}

impl<T> Index<Color> for [T] {
    type Output = T;
    fn index(&self, index: Color) -> &Self::Output {
        &self [index as usize]
    }
}

impl<T> IndexMut<Color> for [T] {
    fn index_mut(&mut self, index: Color) -> &mut Self::Output {
        &mut self [index as usize]
    }
}

impl Display for Color {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Color::White => write!(f, "w"),
            Color::Black => write!(f, "b")
        }
    }
}