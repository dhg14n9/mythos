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
    S(-19, -45), S( -2, -24), S( 11,  -8), S( 20,   5), S( 26,  15), S( 32,  21), S( 37,  25),
    S( 40,  25), S( 42,  29), S( 48,  23), S( 62,  15), S( 89,   7), S( 53,  20), S( 79,   5),
];

#[rustfmt::skip]
pub static ROOK_SEED: [S; 15] = [
    S(-34, -42), S(-22, -10), S(-16,  12), S( -9,  26), S( -5,  35), S(  4,  40), S(  8,  48),
    S( 20,  44), S( 36,  45), S( 48,  45), S( 59,  47), S( 71,  48), S( 80,  50), S( 90,  43),
    S(101,  39),
];

#[rustfmt::skip]
pub static QUEEN_SEED: [S; 28] = [
    S(  0, -102), S( -3, -103), S( -4, -121), S( -6,  -99), S( -5,  -50), S(  0,  -32), S(  1,  -17),
    S( -1,   23), S(  2,   30), S(  6,   39), S(  8,   55), S( 15,   63), S(  7,   73), S( 23,   72),
    S( 21,   86), S( 24,   90), S( 24,  103), S( 55,   75), S( 40,   96), S( 61,   95), S(100,   71),
    S( 82,   90), S(100,   82), S(200,   42), S(189,   34), S(252,    6), S( 71,   90), S(117,   61),
];

#[rustfmt::skip]
pub static KNIGHT_MOBILITY: [S; 9] = [
    S(  5, -59), S( 20,  -8), S( 28,   7), S( 32,  15), S( 43,  15), S( 46,  24), S( 52,  18),
    S( 58,  14), S( 79, -12),
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


