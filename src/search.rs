use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use crate::board::board::Board;
use crate::eval::eval::eval;
use crate::movepicker::MovePicker;
use crate::tables::{BoundType, ThreadData, TransTable};
use crate::types::{Move, Score};

const TC_NODE_CHECK: u64 = 2048;

pub struct TimeControl {
    pub stop: Arc<AtomicBool>,
    pub start: Instant,
    pub soft_lim: Duration,
    pub hard_lim: Duration,
}

impl TimeControl {
    pub fn infinite() -> Self {
        Self {
            stop: Arc::new(AtomicBool::new(false)),
            start: Instant::now(),
            soft_lim: Duration::MAX,
            hard_lim: Duration::MAX,
        }
    }
}

pub struct Search {
    pub time_control: TimeControl,
    pub nodes: u64,
    pub stopped: bool,
    pub silent: bool,
    pub trans_table: TransTable,
    pub thread_data: ThreadData
}

impl Search {
    pub fn new(time_control: TimeControl, trans_table: TransTable, thread_data: ThreadData) -> Self {
        Self { time_control, nodes: 0, stopped: false, silent: false, trans_table, thread_data }
    }

    fn should_stop(&mut self) -> bool {
        if self.stopped {
            return true;
        }
        if (self.nodes & (TC_NODE_CHECK - 1) == 0) &&
            (self.time_control.stop.load(Ordering::Relaxed) || (self.time_control.start.elapsed() > self.time_control.hard_lim))
        {
            self.stopped = true;
        }
        self.stopped
    }

    pub fn qsearch(
        &mut self,
        board: &mut Board,
        mut alpha: i32,
        beta: i32,
        ply: usize
    ) -> i32 {
        self.nodes += 1;
        if self.should_stop() {
            return 0; // search cancelled
        }

        let stm = board.stm();
        let in_check = board.is_check();
        let mut best = -Score::MAX;

        if !in_check {
            best = eval(board);
            if best >= beta {
                return best;
            }
            alpha = alpha.max(best);
        }

        let mut move_picker = MovePicker::new(Move::NULL);
        move_picker.gen_move(board, true);
        move_picker.score_quiet(&self.thread_data, stm, ply);
        move_picker.score_noisy(board);

        if in_check && move_picker.terminal() {
            return -Score::MAX;
        }

        while let Some(mv) = move_picker.next() {
            board.make_move(mv);
            let score = -self.qsearch(board, -beta, -alpha, ply + 1);
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

    pub fn negamax(
        &mut self,
        board: &mut Board,
        depth: usize,
        mut alpha: i32,
        beta: i32,
        ply: usize
    ) -> i32 {
        self.nodes += 1;
        if self.should_stop() {
            return 0; // search cancelled
        }

        let mut tt_move = Move::NULL;
        if let Some((score, best, entry_depth, bound)) = self.trans_table.probe(board.hash()) {
            tt_move = best;
            if entry_depth >= depth {
                match bound {
                    BoundType::Exact => {return score}
                    BoundType::Lower => {if score >= beta {return score}}
                    BoundType::Upper => {if score <= alpha {return score}}
                }
            }
        }

        if depth == 0 {
            return self.qsearch(board, alpha, beta, ply);
        };
        let mut best = -Score::MAX;
        let mut best_move = Move::NULL;

        let stm = board.stm();
        // store quiets that doesn't get cut off to give malus
        let mut failure: [Move; 32] = [Move::NULL; 32];
        let mut n_failed: usize = 0;

        let mut move_picker = MovePicker::new(tt_move);
        move_picker.gen_move(board, false);
        move_picker.score_quiet(&self.thread_data, stm, ply);
        move_picker.score_noisy(board);

        if move_picker.terminal() {
            return if board.is_check() { -Score::MAX } else { Score::ZERO };
        }

        let alpha_orig = alpha;
        let mut i = 0; // move num in move ordering
        while let Some(mv) = move_picker.next() {
            let escaping_check = board.is_check();
            board.make_move(mv);
            let mut score: i32;

            if self.is_reducable(i, depth, ply, mv, board, escaping_check) {
                let reduction = Self::reduction(depth, i);
                score = -self.negamax(board, depth - 1 - reduction, -beta, -alpha, ply + 1);

                // unexpectedly good move
                if score > alpha {
                    score = -self.negamax(board, depth - 1, -beta, -alpha, ply + 1); // full depth search
                }
            } else {
                score = -self.negamax(board, depth - 1, -beta, -alpha, ply + 1);
            }


            board.unmake_move(mv);
            i += 1;

            if score > best {
                best = score;
                best_move = mv;
            }
            alpha = alpha.max(best);

            if alpha >= beta {
                if mv.is_quiet() {
                    // add to killer + history
                    self.thread_data.killer.store(mv, ply);
                    self.thread_data.history.bonus(stm, mv.from(), mv.to(), depth);

                    // malus other moves
                    for i in 0..n_failed {
                        let mv = failure[i];
                        self.thread_data.history.malus(stm, mv.from(), mv.to(), depth);
                    }
                }

                break;
            };

            if n_failed != 32 && mv.is_quiet() {
                failure[n_failed] = mv;
                n_failed += 1;
            }

        }

        if self.stopped {
            return 0;
        }

        let bound =
        if best <= alpha_orig { BoundType::Upper }
        else if best >= beta  { BoundType::Lower }
        else                  { BoundType::Exact };

        let score = if Score::is_mate(best) {
            best - best.signum()
        } else {
            best
        };

        self.trans_table.store(board.hash(), score, best_move, depth, bound);

        score
    }

    // return bestmove + score
    pub fn start_negamax(&mut self, board: &mut Board, depth: usize) -> Option<(Move, i32)> {
        self.nodes += 1;

        if depth == 0 { return None };

        let tt_move = self.trans_table.probe(board.hash()).map_or(Move::NULL, |(_, best, _, _)| best);

        let mut move_picker = MovePicker::new(tt_move);
        move_picker.gen_move(board, false);
        move_picker.score_quiet(&self.thread_data, board.stm(), 0);
        move_picker.score_noisy(board);

        if move_picker.terminal() {
            return None;
        }

        let mut best = (Move::NULL, -Score::INF);

        while let Some(mv) = move_picker.next() {
            board.make_move(mv);
            let score = -self.negamax(board, depth - 1, -Score::INF, -best.1, 1);
            board.unmake_move(mv);
            if score > best.1 {
                best = (mv, score)
            }
        }

        Some(best)
    }

    // iterative deepening
    pub fn iterative(&mut self, board: &mut Board, max_depth: usize) -> (Move, i32) {

        let mut best = {
            let mut picker = MovePicker::new(Move::NULL);
            picker.gen_move(board, false);
            (picker.random(board.hash()), 0)
        };

        for depth in 1..=max_depth {
            if self.time_control.start.elapsed() > self.time_control.soft_lim {
                break;
            }

            let result = self.start_negamax(board, depth);

            if self.stopped {
                break;
            }
            if let Some(r) = result {
                best = r
            }

            // info
            if !self.silent {
                let ellapsed = self.time_control.start.elapsed();
                let nps = (self.nodes as f64 / ellapsed.as_secs_f64().max(f64::EPSILON)) as u64;
                println!(
                    "info depth {depth} score cp {} nodes {} nps {nps} time {} pv {}",
                    best.1, self.nodes, ellapsed.as_millis(), best.0
                );
            }
        }
        best
    }

    // check if move is reducable, i is move number in move ordering
    fn is_reducable(&self, i: usize, depth: usize, ply: usize, mv: Move, board: &mut Board, escaping_check: bool) -> bool {
        if i < 4 { return false }
        if depth < 3 { return false }
        if mv.is_capture() { return false }
        if mv.is_promotion() { return false }
        let (k1, k2) = self.thread_data.killer.probe(ply);
        if k1 == mv || k2 == mv {
            return false
        }
        if board.is_check() { return false }
        if escaping_check { return false }
        true
    }

    fn reduction(depth: usize, i: usize) -> usize {
        ((0.75 + (depth as f64).ln() * (i as f64).ln() / 2.25) as usize).min(depth - 2)
    }

}


