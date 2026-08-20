use crate::nnue::HL;
use crate::types::{Color, Piece, Square};

#[repr(C, align(64))]
pub struct Accumulator([i16; HL]);

pub fn feature_index(perspective: Color, piece: Piece, square: Square) -> usize {
    (piece.piece_type() as usize) * 64 + (square.relative_to(perspective) as usize) + if perspective == piece.color() { 0 } else { 384 }
}
