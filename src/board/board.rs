use std::fmt;
use crate::types;
use crate::types::{Bitboard, Castling, Color, Piece, PieceType, Square};

#[derive(Copy, Clone)]
pub struct StateInfo {
    pub castling_right: Castling,
    pub en_passant: Square,
    pub half_move: u16,
    pub hash: u64
}

#[derive(Clone)]
pub struct Board {
    piece_type_bb: [Bitboard; PieceType::NUM],
    color_bb: [Bitboard; Color::NUM],
    mailbox: [Piece; Square::NUM],

    side_to_move: Color,
    castling_right: Castling,
    en_passant: Square,
    half_move: u16,
    full_move: usize,
    zobrist: u64,
    game_ply: usize,
    piece_count: [u8; Piece::NUM],
}
