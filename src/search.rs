use crate::board::board::Board;
use crate::eval::eval::eval;
use crate::movepicker::MovePicker;
use crate::types::Score;

fn negamax(board: &mut Board, depth: usize, mut alpha: i32, mut beta: i32) -> i32 {
    if depth == 0 {
        return eval(board);
    };
    let mut best = -Score::MAX;

    let mut move_picker = MovePicker::new();
    move_picker.gen_move(board);

    while let Some(mv) = move_picker.next(board) {
        board.make_move(mv);
        let score = -Score::score_color(negamax(board, depth - 1, -beta, -alpha), board.stm());
        board.unmake_move(mv);
        best = best.max(score);
        alpha = alpha.max(best);

        if alpha >= beta {
            break;
        };
    }
    best
}

pub fn start_negamax(board: &mut Board, depth: usize) -> i32 {
    negamax(board, depth, -Score::INF, Score::INF)
}
