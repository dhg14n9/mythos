pub mod square;
pub mod color;
pub mod bitboard;
pub mod castling;
pub mod moves;
pub mod piece;
pub mod move_list;
pub mod score;
pub mod zobrist;

use std::ops::{Index, IndexMut};
pub use square::*;
pub use color::*;
pub use bitboard::*;
pub use castling::*;
pub use moves::*;
pub use piece::*;
pub use move_list::*;
pub use score::*;
pub use zobrist::*; 


#[derive(Copy, Clone)]
pub enum Direction {
    Up = 0, Down, Left, Right,
    UpLeft, UpRight, DownLeft, DownRight
}

impl<T> Index<Direction> for [T] {
    type Output = T;

    fn index(&self, index: Direction) -> &Self::Output {
        &self [index as usize]
    }
}

impl<T> IndexMut<Direction> for [T] {
    fn index_mut(&mut self, index: Direction) -> &mut Self::Output {
        &mut self [index as usize]
    }
}