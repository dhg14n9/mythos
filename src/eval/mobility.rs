use crate::board::board::Board;
use crate::board::lookup::{bishop_attack, knight_attack, queen_attack, rook_attack};
use crate::eval::spread;
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