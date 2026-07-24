use crate::board::board::Board;
use crate::board::lookup::{bishop_attack, knight_attack, queen_attack, rook_attack};
use crate::eval::{spread, S};
use crate::types::{Bitboard, Color, Direction, Piece, PieceType, Square};

fn pawn_attack(pawn_bb: Bitboard, color: Color) -> Bitboard {
    let result = pawn_bb.shifted(match color {
        Color::White => {Direction::Up}
        Color::Black => {Direction::Down}
    });
    spread(result)
}

fn rook_mobility(square: Square, board: &Board, them_pawn_attack: Bitboard) -> Bitboard {
    debug_assert!(board.piece_at(square).piece_type() == PieceType::Rook);

    let us = board.piece_at(square).color();
    let occ = board.occ()
        ^ board.piece_bb(Piece::new(us, PieceType::Queen))
        ^ board.piece_bb(Piece::new(us, PieceType::Rook));

    rook_attack(occ, square) & (!them_pawn_attack) & (!board.color_bb(us))
}

fn bishop_mobility(square: Square, board: &Board, them_pawn_attack: Bitboard) -> Bitboard {
    debug_assert!(board.piece_at(square).piece_type() == PieceType::Bishop);

    let us = board.piece_at(square).color();
    let occ = board.occ()
        ^ board.piece_bb(Piece::new(us, PieceType::Queen))
        ^ board.piece_bb(Piece::new(us, PieceType::Bishop));

    bishop_attack(occ, square) & (!them_pawn_attack) & (!board.color_bb(us))
}

fn queen_mobility(square: Square, board: &Board, them_pawn_attack: Bitboard) -> Bitboard {
    debug_assert!(board.piece_at(square).piece_type() == PieceType::Queen);

    let us = board.piece_at(square).color();
    let occ = board.occ()
        ^ board.piece_bb(Piece::new(us, PieceType::Queen))
        ^ board.piece_bb(Piece::new(us, PieceType::Rook))
        ^ board.piece_bb(Piece::new(us, PieceType::Bishop));

    queen_attack(occ, square) & (!them_pawn_attack) & (!board.color_bb(us))
}

fn knight_mobility(square: Square, board: &Board, them_pawn_attack: Bitboard) -> Bitboard {
    debug_assert!(board.piece_at(square).piece_type() == PieceType::Knight);

    knight_attack(square) & (!them_pawn_attack) & (!board.color_bb(board.piece_at(square).color()))
}

// Reckless's Mobility Seed
#[rustfmt::skip]
static BISHOP_SEED: [S; 14] = [
    S(  0,   0), S(-47, -57), S(-34, -44), S(-26, -11), S(-14,  -2), S( -8,   7), S(  4,  23),
    S( 13,  29), S( 20,  40), S( 21,  44), S( 26,  51), S( 29,  47), S( 30,  47), S( 57,  37),
];

#[rustfmt::skip]
static ROOK_SEED: [S; 15] = [
    S(  0,   0), S(  0,   0), S(-85,  43), S(-76,  56), S(-69,  63), S(-64,  66), S(-61,  70),
    S(-58,  75), S(-52,  77), S(-45,  80), S(-38,  84), S(-31,  86), S(-26,  90), S(-17,  93),
    S(-13,  93),
];

#[rustfmt::skip]
static QUEEN_SEED: [S; 28] = [
    S(  0,   0), S(  0,   0), S(  0,   0), S( -7,  -1), S(-31, -27), S(  5, -24), S(  5,   2),
    S(  3,  73), S(  8,  90), S(  8, 111), S( 12, 118), S( 15, 130), S( 17, 142), S( 20, 145),
    S( 24, 151), S( 24, 161), S( 25, 168), S( 27, 176), S( 27, 183), S( 28, 189), S( 30, 198),
    S( 33, 198), S( 35, 201), S( 45, 198), S( 49, 195), S( 69, 189), S(125, 156), S(132, 167),
];

#[rustfmt::skip]
pub static KNIGHT_MOBILITY: [S; 9] = [
    S(  0,   0), S(  0,   0), S(-38, -42), S(-25, -25), S(-14, -10), S( -5,   2), S(  4,  14),
    S( 13,  24), S( 22,  32),
];