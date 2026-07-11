use crate::board::board::Board;
use crate::eval::eval::eval;
use crate::movepicker::MovePicker;
use crate::types::{Move, Score};

fn negamax(board: &mut Board, depth: usize, mut alpha: i32, beta: i32) -> i32 {
    if depth == 0 {
        return eval(board);
    };
    let mut best = -Score::MAX;

    let mut move_picker = MovePicker::new();
    move_picker.gen_move(board);

    if move_picker.terminal() {
        return if board.is_check() { -Score::MAX } else { Score::ZERO };
    }

    while let Some(mv) = move_picker.next() {
        board.make_move(mv);
        let score = -negamax(board, depth - 1, -beta, -alpha);
        board.unmake_move(mv);
        best = best.max(score);
        alpha = alpha.max(best);

        if alpha >= beta {
            break;
        };
    }

    if Score::is_mate(best) {
        best - best.signum()
    } else {
        best
    }
}

// return bestmove + score
fn start_negamax(board: &mut Board, depth: usize) -> Option<(Move, i32)> {
    if depth == 0 { return None };

    let mut move_picker = MovePicker::new();
    move_picker.gen_move(board);

    if move_picker.terminal() {
        return None;
    }
    
    let mut best = (Move::NULL, -Score::score_color(Score::INF, board.stm())); 

    while let Some(mv) = move_picker.next() { 
        board.make_move(mv); 
        let score = -negamax(board, depth, -Score::INF, -best.1); 
        board.unmake_move(mv); 
        if score >= best.1 { 
            best = (mv, score)
        }
    }
    
    Some(best)
}

pub fn iterative(board: &mut Board, depth: usize) -> Option<(Move, i32)> {
    todo!()
}
