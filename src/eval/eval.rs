use crate::board::board::Board;
use crate::eval::S;
use crate::eval::piece_square::psqt;
use crate::types::Score;

fn taper(score: S, phase: i32) -> i32 {
    let mg_phase = phase.min(Board::GAME_PHASE_MAX);
    let eg_phase = Board::GAME_PHASE_MAX - mg_phase;
    (score.0 * mg_phase + score.1 * eg_phase) / Board::GAME_PHASE_MAX
}

pub fn eval(board: &Board) -> i32 {
    let score = psqt(board);
    Score::score_color(taper(score, board.phase()), board.stm())
}
