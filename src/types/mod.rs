pub mod bitboard;
pub mod castling;
pub mod color;
pub mod move_list;
pub mod moves;
pub mod piece;
pub mod score;
pub mod square;
pub mod uninit_array;
pub mod zobrist;

pub use bitboard::*;
pub use castling::*;
pub use color::*;
pub use move_list::*;
pub use moves::*;
pub use piece::*;
pub use score::*;
pub use square::*;
use std::ops::{Index, IndexMut};
pub use zobrist::*;

#[derive(Copy, Clone)]
pub enum Direction {
    Up = 0,
    Down,
    Left,
    Right,
    UpLeft,
    UpRight,
    DownLeft,
    DownRight,
}

impl<T> Index<Direction> for [T] {
    type Output = T;

    fn index(&self, index: Direction) -> &Self::Output {
        &self[index as usize]
    }
}

impl<T> IndexMut<Direction> for [T] {
    fn index_mut(&mut self, index: Direction) -> &mut Self::Output {
        &mut self[index as usize]
    }
}
