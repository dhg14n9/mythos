use crate::board::board::Board;
use crate::eval::piece_square::{pst_eval, raw_eval};

pub fn eval(board: &Board) -> i32 {
    let mut raw = 0;
    let phase = board.phase();
    raw += pst_eval(board, phase) + raw_eval(board, phase);
    raw
}