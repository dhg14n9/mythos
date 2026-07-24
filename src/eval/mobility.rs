use crate::eval::spread;
use crate::types::{Bitboard, Color, Direction};

fn pawn_attack(pawn_bb: Bitboard, color: Color) -> Bitboard {
    let result = pawn_bb.shifted(match color {
        Color::White => {Direction::Up}
        Color::Black => {Direction::Down}
    });
    spread(result)
}

