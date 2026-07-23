use crate::board::board::Board;
use crate::eval::{s_color, S};
use crate::eval::piece_square::psqt;
use crate::types::{Color, Piece, PieceType, Score};

fn taper(score: S, phase: i32) -> i32 {
    let mg_phase = phase.min(Board::GAME_PHASE_MAX);
    let eg_phase = Board::GAME_PHASE_MAX - mg_phase;
    (score.0 * mg_phase + score.1 * eg_phase) / Board::GAME_PHASE_MAX
}

pub fn eval(board: &Board) -> i32 {
    let score =
              psqt(board)
            + tempo(board.stm())
            + bishop_pair(board)
        ;
    Score::score_color(taper(score, board.phase()), board.stm())
}

fn tempo(stm: Color) -> S {
    const TEMPO_BONUS: S = S(20, 10);

    s_color(TEMPO_BONUS, stm)
}

fn bishop_pair(board: &Board) -> S {
    const BISHOP_PAIR_BONUS: S = S(25, 45);

    let mut result = S(0, 0);
    for color in Color::ALL {
        if board.piece_bb(Piece::new(color, PieceType::Bishop)).pop_count() >= 2 {
            result += s_color(BISHOP_PAIR_BONUS, color);
        }
    }

    result
}