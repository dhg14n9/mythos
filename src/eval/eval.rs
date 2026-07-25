use crate::board::board::Board;
use crate::eval::{s_color, S};
use crate::eval::king_safety::king_safety;
use crate::eval::mobility::mobility;
use crate::eval::pawn::pawns;
use crate::eval::piece_square::psqt;
use crate::types::{Color, Piece, PieceType, Score};

fn taper(score: S, phase: i32) -> i32 {
    let mg_phase = phase.min(Board::GAME_PHASE_MAX);
    let eg_phase = Board::GAME_PHASE_MAX - mg_phase;
    (score.0 * mg_phase + score.1 * eg_phase) / Board::GAME_PHASE_MAX
}

pub const TEMPO_BONUS: S = S(20, 10);
pub const BISHOP_PAIR_BONUS: S = S(25, 45);

// White eval score
pub fn eval_score(board: &Board) -> S {
              psqt(board)
            + tempo(board.stm())
            + bishop_pair(board)
            + pawns(board)
            + mobility(board)
            + king_safety(board)
}

pub fn eval(board: &Board) -> i32 {
    Score::score_color(taper(eval_score(board), board.phase()), board.stm())
}

pub fn s_eval(board: &Board) -> i32 {
    taper(eval_score(board), board.phase())
}

pub fn tempo(stm: Color) -> S {
    s_color(TEMPO_BONUS, stm)
}

pub fn bishop_pair(board: &Board) -> S {
    let mut result = S(0, 0);
    for color in Color::ALL {
        if board.piece_bb(Piece::new(color, PieceType::Bishop)).pop_count() >= 2 {
            result += s_color(BISHOP_PAIR_BONUS, color);
        }
    }

    result
}