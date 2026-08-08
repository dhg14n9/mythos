use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use crate::board::board::Board;
use crate::eval::eval::eval;
use crate::movepicker::{see, MovePicker};
use crate::tables::{BoundType, ContKey, ThreadData, TransTable, MAX_PLY};
use crate::types::{Color, Move, PieceType, Score};

const TC_NODE_CHECK: u64 = 2048;

// track stable best move
struct StableTracker {
    mv: Move,
    stable_iteration: usize

}

impl StableTracker {
    pub fn new() -> Self {
        Self {
            mv: Move::NULL,
            stable_iteration: 0
        }
    }

    fn extension(&self) -> f32 {
        match self.stable_iteration {
            0 => 2.5,
            1 => 1.8,
            2 => 1.4,
            3 => 1.2,
            4 => 1.0,
            _ => 0.9
        }
    }

    pub fn update(&mut self, mv: Move, depth: usize, tc: &mut TimeControl) {
        if mv == self.mv {
            self.stable_iteration += 1;
        } else {
            self.mv = mv;
            self.stable_iteration = 0;
        }

        if tc.soft_base != Duration::MAX  && depth > 8 {
            tc.soft_lim = tc.soft_base.mul_f32(self.extension()).min(tc.hard_lim);
        }
    }
}

pub struct PvTable {
    table: Box<[[Move; MAX_PLY + 1]]>,
    len: [usize; MAX_PLY + 1]
}

impl PvTable {
    pub fn new() -> Self {
        Self {
            table: vec![[Move::NULL; MAX_PLY + 1]; MAX_PLY + 1].into_boxed_slice(),
            len: [0; MAX_PLY + 1]
        }
    }

    pub fn clear(&mut self, ply: usize) {
        self.len[ply] = 0
    }

    pub fn update(&mut self, ply: usize, mv: Move) {
        self.table[ply][0] = mv;
        self.len[ply] = self.len[ply + 1] + 1;
        for i in 0..self.len[ply + 1] {
            self.table[ply][i + 1] = self.table[ply + 1][i]
        }
    }

    pub fn get_line(&self, ply: usize) -> &[Move] {
        &self.table[ply][..self.len[ply]]
    }
}


pub struct TimeControl {
    pub stop: Arc<AtomicBool>,
    pub start: Instant,
    pub soft_lim: Duration,
    pub hard_lim: Duration,
    pub soft_base: Duration
}

impl TimeControl {
    pub fn infinite() -> Self {
        Self {
            stop: Arc::new(AtomicBool::new(false)),
            start: Instant::now(),
            soft_lim: Duration::MAX,
            hard_lim: Duration::MAX,
            soft_base: Duration::MAX
        }
    }
}

pub struct Search {
    pub time_control: TimeControl,
    pub nodes: u64,
    pub stopped: bool,
    pub silent: bool,
    pub trans_table: TransTable,
    pub thread_data: ThreadData,
    pub root_depth: usize,
    pub pv_table: PvTable,
    pub cont_stack: Box<[Option<ContKey>; MAX_PLY]>
}

impl Search {
    pub fn new(time_control: TimeControl, trans_table: TransTable, thread_data: ThreadData) -> Self {
        Self {
            time_control,
            nodes: 0,
            stopped: false,
            silent: false,
            trans_table,
            thread_data,
            root_depth: 0,
            pv_table: PvTable::new(),
            cont_stack: Box::from([None; MAX_PLY])
        }
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

    pub fn qsearch<const PV: bool>(
        &mut self,
        board: &mut Board,
        mut alpha: i32,
        beta: i32,
        ply: usize
    ) -> i32 {
        self.nodes += 1;
        self.pv_table.clear(ply);

        if self.should_stop() {
            return 0; // search cancelled
        }

        if ply >= MAX_PLY - 1 {
            return eval(board);
        }

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

        let prev = if ply > 0 { self.cont_stack[ply - 1] } else { None };
        move_picker.score_quiet(&board, &self.thread_data, ply, prev);
        move_picker.score_noisy(board);

        if in_check && move_picker.terminal() {
            return -Score::MAX;
        }

        while let Some(mv) = move_picker.next(board) {

            // see pruning
            if !in_check && !mv.is_promotion() && move_picker.is_bad() {
                continue;
            }

            self.cont_stack[ply] = Some(ContKey {
                ptr: self.thread_data.continuation
                    .pth_ptr(in_check, mv.is_capture(), board.piece_at(mv.from()), mv.to())
            });

            board.make_move(mv);
            let score = -self.qsearch::<PV>(board, -beta, -alpha, ply + 1);
            board.unmake_move(mv);
            best = best.max(score);

            if best > alpha {
                alpha = best;

                if PV {
                    self.pv_table.update(ply, mv);
                }
            }

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

    pub fn negamax<const PV: bool>(
        &mut self,
        board: &mut Board,
        depth: usize,
        mut alpha: i32,
        beta: i32,
        ply: usize,
        allow_null: bool
    ) -> i32 {
        self.nodes += 1;
        self.pv_table.clear(ply);

        if self.should_stop() {
            return 0; // search cancelled
        }

        if ply > 0 && board.is_draw() {
            return Score::ZERO;
        }

        if ply >= MAX_PLY - 1 {
            return eval(board);
        }

        let mut tt_move = Move::NULL;
        if let Some((score, best, entry_depth, bound)) = self.trans_table.probe(board.hash()) {
            tt_move = best;
            if entry_depth >= depth && !PV {
                match bound {
                    BoundType::Exact => {return score}
                    BoundType::Lower => {if score >= beta {return score}}
                    BoundType::Upper => {if score <= alpha {return score}}
                }
            }
        }

        if depth == 0 {
            return self.qsearch::<PV>(board, alpha, beta, ply);
        };

        let static_eval = eval(board);

        if allow_null && self.should_nmp(beta, depth, board, static_eval) {
            self.cont_stack[ply] = None;

            board.make_null_move();
            let score = -self.negamax::<false>(board, (depth - 1).saturating_sub(Self::nmp_reduction(depth)), -beta, -beta + 1, ply + 1, false);
            board.unmake_null_move();

            if score >= beta { return score }
        }

        if self.should_rfp(board, beta, depth) && static_eval > beta + Self::rfp_margin(depth) {
            return static_eval
        }

        let mut best = -Score::MAX;
        let mut best_move = Move::NULL;

        let stm = board.stm();
        let in_check = board.is_check();
        // store quiets that doesn't get cut off to give malus
        let mut failure: [Move; 32] = [Move::NULL; 32];
        let mut n_failed: usize = 0;

        let mut move_picker = MovePicker::new(tt_move);
        move_picker.gen_move(board, false);

        let prev = if ply > 0 { self.cont_stack[ply - 1] } else { None };
        move_picker.score_quiet(&board, &self.thread_data, ply, prev);
        move_picker.score_noisy(board);

        if move_picker.terminal() {
            return if in_check { -Score::MAX } else { Score::ZERO };
        }

        let alpha_orig = alpha;
        let mut i = 0; // move num in move ordering
        while let Some(mv) = move_picker.next(board) {

            if !PV && !in_check && best > -Score::MAX {
                if Self::should_see_prune(board, depth, mv) {
                    continue;
                }

                if mv.is_quiet() && (Self::should_lmp(depth, i) || Self::should_futility(depth, static_eval, alpha))
                {
                    move_picker.skip_quiets();
                    continue;
                }
            }

            self.cont_stack[ply] = Some(ContKey {
                ptr: self.thread_data.continuation
                    .pth_ptr(in_check, mv.is_capture(), board.piece_at(mv.from()), mv.to())
            });

            board.make_move(mv);
            let give_check = board.is_check();

            let mut extension = 0;
            // temporarily scrap this check extension
            // if board.is_check() && ply < self.root_depth / 2 {
            //     extension += 1;
            // }
            let new_depth = depth - 1 + extension;

            let mut score;

            if i == 0 {
                score = -self.negamax::<PV>(board, new_depth, -beta, -alpha, ply + 1, true);
            } else {
                let r = if self.should_lmr(i, depth, ply, mv, give_check, in_check) { Self::lmr_reduction(depth, i) } else { 0 };
                let r = r.min(new_depth.saturating_sub(1));
                let reduced_depth = new_depth - r;

                score = -self.negamax::<false>(board, reduced_depth, -alpha - 1, -alpha, ply + 1, true);

                // wrong reduction
                if score > alpha && reduced_depth < new_depth {
                    score = -self.negamax::<false>(board, new_depth, -alpha - 1, -alpha, ply + 1, true);
                }

                // new PV
                if score > alpha && score < beta {
                    score = -self.negamax::<PV>(board, new_depth, -beta, -alpha, ply + 1, true);
                }
            }

            board.unmake_move(mv);
            i += 1;

            if score > best {
                best = score;
                best_move = mv;
            }

            if best > alpha {
                alpha = best;

                if PV {
                    self.pv_table.update(ply, best_move);
                }
            }

            if alpha >= beta {
                if mv.is_quiet() {
                    let quiet_bonus = (180 * depth as i32).min(1750) - 70;
                    let quiet_malus = (170 * depth as i32).min(1100) - 40 - 30 * n_failed as i32;
                    let cont_bonus  = (100 * depth as i32).min(1100) - 70;
                    let cont_malus  = (400 * depth as i32).min(950) - 50 - 20 * n_failed as i32;

                    // add to killer + history
                    self.thread_data.killer.store(mv, ply);
                    self.thread_data.history.update(stm, mv.from(), mv.to(), quiet_bonus);
                    if let Some(prev) = prev {
                        self.thread_data.continuation.update(prev.ptr, board.piece_at(mv.from()), mv.to(), cont_bonus);
                    }

                    for j in 0..n_failed {
                        let failed = failure[j];
                        let denom = 1024 + 45 * j as i32;
                        let scale = 1024 * 1024 / (denom * denom / 1024);

                        self.thread_data.history.update(stm, failed.from(), failed.to(), -quiet_malus * scale / 1024);
                        if let Some(prev) = prev {
                            self.thread_data.continuation.update(prev.ptr, board.piece_at(failed.from()), failed.to(), -cont_malus * scale / 1024);
                        }
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
    pub fn start_negamax(&mut self, board: &mut Board, depth: usize, alpha: i32, beta: i32) -> Option<(Move, i32)> {
        self.nodes += 1;
        self.pv_table.clear(0);

        if depth == 0 { return None };

        let tt_move = self.trans_table.probe(board.hash()).map_or(Move::NULL, |(_, best, _, _)| best);

        let mut move_picker = MovePicker::new(tt_move);
        move_picker.gen_move(board, false);
        move_picker.score_quiet(&board, &self.thread_data, 0, None);
        move_picker.score_noisy(board);

        if move_picker.terminal() {
            return None;
        }

        let mut best = (Move::NULL, -Score::INF);
        let mut alpha = alpha;

        while let Some(mv) = move_picker.next(board) {
            self.cont_stack[0] = Some(ContKey {
                ptr: self.thread_data.continuation
                    .pth_ptr(board.is_check(), mv.is_capture(), board.piece_at(mv.from()), mv.to())
            });

            board.make_move(mv);
            let score = -self.negamax::<true>(board, depth - 1, -beta, -alpha, 1, true);
            board.unmake_move(mv);

            if score > best.1 {
                best = (mv, score)
            }
            if score > alpha {
                alpha = score;
                self.pv_table.update(0, best.0)
            }

            if alpha >= beta {
                break;
            }
        }

        if Score::is_mate(best.1) {
            best.1 = best.1 - best.1.signum()
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

        let mut alpha = -Score::INF;
        let mut beta = Score::INF;
        let mut best_pv: Vec<Move> = Vec::new();
        let mut stable_tracker = StableTracker::new();

        for depth in 1..=max_depth {
            if self.time_control.start.elapsed() > self.time_control.soft_lim {
                break;
            }
            self.root_depth = depth;
            let mut alpha_tries: usize = 0;
            let mut beta_tries: usize = 0;

            let mut result = match self.start_negamax(board, depth, alpha, beta) {
                Some(r) => r,
                None => break, // no legal moves at the root
            };

            while !self.stopped && (alpha >= result.1 || beta <= result.1) {
                if alpha >= result.1 {
                    alpha -= match alpha_tries {
                        0 => 30,
                        1 => 120,
                        2 => 200,
                        _ => alpha + Score::INF
                    };
                    alpha_tries += 1;
                }
                if beta <= result.1 {
                    beta += match beta_tries {
                        0 => 30,
                        1 => 120,
                        2 => 200,
                        _ => Score::INF - beta
                    };
                    beta_tries += 1;
                }
                result = match self.start_negamax(board, depth, alpha, beta) {
                    Some(r) => r,
                    None => break,
                };
            }

            if self.stopped {
                break;
            }

            stable_tracker.update(result.0, depth, &mut self.time_control);

            alpha = result.1 - 30;
            beta = result.1 + 30;
            best = result;
            best_pv = Vec::from(self.pv_table.get_line(0));

            // info
            if !self.silent {
                let score = if Score::is_mate(best.1) {
                    format!("mate {}", Score::mate_distance(best.1))
                } else {
                    format!("cp {}", best.1)
                };
                let ellapsed = self.time_control.start.elapsed();
                let nps = (self.nodes as f64 / ellapsed.as_secs_f64().max(f64::EPSILON)) as u64;
                println!(
                    "info depth {depth} score {} nodes {} nps {nps} time {} pv {}",
                    score, // is mate print "mate N", not mate print cp score
                    self.nodes,
                    ellapsed.as_millis(),
                    best_pv.iter()
                           .map(|x| x.to_string())
                           .collect::<Vec<_>>()
                           .join(" ")
                );
            }
        }
        best
    }

    // check if move is reducable, i is move number in move ordering
    fn should_lmr(&self, i: usize, depth: usize, ply: usize, mv: Move, is_check: bool, escaping_check: bool) -> bool {
        if i < 4 { return false }
        if depth < 3 { return false }
        if mv.is_capture() { return false }
        if mv.is_promotion() { return false }
        let (k1, k2) = self.thread_data.killer.probe(ply);
        if k1 == mv || k2 == mv {
            return false
        }
        if is_check { return false }
        if escaping_check { return false }
        true
    }

    // allow null move pruning
    fn should_nmp(&self, beta: i32, depth: usize, board: &Board, static_eval: i32) -> bool {
        if depth < 3 { return false }
        if board.is_check() { return false }
        if Score::is_mate(beta) { return false }
        if static_eval < beta { return false }
        if !has_non_pawn_piece(board, board.stm()) { return false }
        if Score::is_mate(beta) { return false }
        true
    }

    fn should_rfp(&self, board: &Board, beta: i32, depth: usize) -> bool {
        if board.is_check() { return false }
        if Score::is_mate(beta) { return false }
        if depth > 5 { return false }
        true
    }

    fn should_lmp(depth: usize, i: usize) -> bool {
        (depth <= 8) && (i >= ((3 + depth * depth) * 3 / 2))
    }

    fn should_futility(depth: usize, static_eval: i32, alpha: i32) -> bool {
        (depth <= 6) && ((static_eval + 100 * depth as i32) <= alpha) && !Score::is_mate(alpha)
    }

    fn should_see_prune(board: &Board, depth: usize, mv: Move) -> bool {
        (depth < 5) && !see(board, mv, Self::see_threshold(depth, mv))
    }

    fn lmr_reduction(depth: usize, i: usize) -> usize {
        (0.75 + (depth as f64).ln() * (i as f64).ln() / 2.25) as usize
    }

    fn nmp_reduction(depth: usize) -> usize {
        3 + depth / 3
    }

    fn rfp_margin(depth: usize) -> i32 {
        150 * depth as i32
    }

    fn see_threshold(depth: usize, mv: Move) -> i32 {
        (depth as i32) * if mv.is_quiet() { -50 } else { -100 }
    }

}

fn has_non_pawn_piece(board: &Board, color: Color) -> bool {
    let bb =
            board.piece_type_bb(PieceType::Queen) |
            board.piece_type_bb(PieceType::Bishop) |
            board.piece_type_bb(PieceType::Knight) |
            board.piece_type_bb(PieceType::Rook);

    !(bb & board.color_bb(color)).is_empty()

}
