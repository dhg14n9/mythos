use crate::board::board::Board;
use crate::board::lookup::{bishop_attack, knight_attack, queen_attack, rook_attack};
use crate::eval::{s_color, spread, S};
use crate::types::{Bitboard, Color, Direction, Piece, PieceType, Square};

pub(crate) fn pawn_attack(pawn_bb: Bitboard, color: Color) -> Bitboard {
    let result = pawn_bb.shifted(match color {
        Color::White => {Direction::Up}
        Color::Black => {Direction::Down}
    });
    spread(result)
}

pub(crate) fn rook_mobility(square: Square, board: &Board, them_pawn_attack: Bitboard) -> Bitboard {
    debug_assert!(board.piece_at(square).piece_type() == PieceType::Rook);

    let us = board.piece_at(square).color();
    let occ = board.occ()
        ^ board.piece_bb(Piece::new(us, PieceType::Queen))
        ^ board.piece_bb(Piece::new(us, PieceType::Rook));

    rook_attack(occ, square) & (!them_pawn_attack) & (!board.color_bb(us))
}

pub(crate) fn bishop_mobility(square: Square, board: &Board, them_pawn_attack: Bitboard) -> Bitboard {
    debug_assert!(board.piece_at(square).piece_type() == PieceType::Bishop);

    let us = board.piece_at(square).color();
    let occ = board.occ()
        ^ board.piece_bb(Piece::new(us, PieceType::Queen))
        ^ board.piece_bb(Piece::new(us, PieceType::Bishop));

    bishop_attack(occ, square) & (!them_pawn_attack) & (!board.color_bb(us))
}

pub(crate) fn queen_mobility(square: Square, board: &Board, them_pawn_attack: Bitboard) -> Bitboard {
    debug_assert!(board.piece_at(square).piece_type() == PieceType::Queen);

    let us = board.piece_at(square).color();
    let occ = board.occ()
        ^ board.piece_bb(Piece::new(us, PieceType::Queen))
        ^ board.piece_bb(Piece::new(us, PieceType::Rook))
        ^ board.piece_bb(Piece::new(us, PieceType::Bishop));

    queen_attack(occ, square) & (!them_pawn_attack) & (!board.color_bb(us))
}

pub(crate) fn knight_mobility(square: Square, board: &Board, them_pawn_attack: Bitboard) -> Bitboard {
    debug_assert!(board.piece_at(square).piece_type() == PieceType::Knight);

    knight_attack(square) & (!them_pawn_attack) & (!board.color_bb(board.piece_at(square).color()))
}

// Placeholder mobility values: small, self-scaled, monotonic, centered near each
// piece's typical mobility so a balanced position nets ~0. Tune with SPRT.
#[rustfmt::skip]
pub static BISHOP_SEED: [S; 14] = [
    S(-30, -35), S(-22, -26), S(-15, -18), S( -9, -11), S( -4,  -5), S(  0,   0), S(  4,   5),
    S(  8,  10), S( 12,  15), S( 15,  19), S( 18,  23), S( 21,  27), S( 24,  31), S( 27,  35),
];

#[rustfmt::skip]
pub static ROOK_SEED: [S; 15] = [
    S(-24, -30), S(-18, -22), S(-13, -15), S( -8,  -9), S( -4,  -4), S(  0,   0), S(  3,   5),
    S(  6,  10), S(  9,  15), S( 11,  20), S( 13,  25), S( 15,  30), S( 17,  36), S( 19,  42),
    S( 21,  48),
];

#[rustfmt::skip]
pub static QUEEN_SEED: [S; 28] = [
    S(-20, -26), S(-17, -22), S(-14, -18), S(-11, -14), S( -8, -11), S( -6,  -8), S( -4,  -5),
    S( -2,  -2), S(  0,   0), S(  1,   2), S(  3,   5), S(  4,   8), S(  5,  11), S(  6,  14),
    S(  8,  17), S(  9,  20), S( 10,  23), S( 11,  26), S( 12,  29), S( 13,  32), S( 14,  35),
    S( 15,  38), S( 16,  41), S( 17,  44), S( 18,  47), S( 19,  50), S( 20,  53), S( 21,  56),
];

#[rustfmt::skip]
pub static KNIGHT_MOBILITY: [S; 9] = [
    S(-30, -30), S(-22, -22), S(-14, -14), S( -7,  -7), S(  0,   0), S(  6,   6), S( 12,  12),
    S( 17,  17), S( 22,  22),
];

pub fn mobility(board: &Board) -> S {
    let mut result = S(0, 0);

    for color in Color::ALL {
        let them_pawn_attack = pawn_attack(board.piece_bb(Piece::new(!color, PieceType::Pawn)), !color);

        // queen
        for sq in board.piece_bb(Piece::new(color, PieceType::Queen)) {
            result += s_color(QUEEN_SEED[queen_mobility(sq, board, them_pawn_attack).pop_count()], color)
        }

        // rook
        for sq in board.piece_bb(Piece::new(color, PieceType::Rook)) {
            result += s_color(ROOK_SEED[rook_mobility(sq, board, them_pawn_attack).pop_count()], color)
        }

        // knight
        for sq in board.piece_bb(Piece::new(color, PieceType::Knight)) {
            result += s_color(KNIGHT_MOBILITY[knight_mobility(sq, board, them_pawn_attack).pop_count()], color)
        }

        // bishop
        for sq in board.piece_bb(Piece::new(color, PieceType::Bishop)) {
            result += s_color(BISHOP_SEED[bishop_mobility(sq, board, them_pawn_attack).pop_count()], color)
        }
    }

    result
}


