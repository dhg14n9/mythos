use crate::board::board::Board;
use crate::eval::piece_square::{pst_eval, raw_eval};
use crate::types::Score;

pub fn eval(board: &Board) -> i32 {
    let phase = board.phase();
    let raw = pst_eval(board, phase) + raw_eval(board, phase);
    Score::score_color(raw, board.stm())
}
