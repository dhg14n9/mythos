use crate::board::board::Board;
use crate::tables::{ContKey, ThreadData, CONT_LEN};
use crate::types::{Bitboard, Color, MAX_LIST_LENGTH, Move, MoveList, Piece, PieceType, Square};

const KILLER1_SCORE: i32 = 1_000_000;
const KILLER2_SCORE: i32 = 900_000;

// yield order
#[derive(Copy, Clone, PartialEq)]
enum Stage {
    TtMove,
    GoodNoisy,
    Quiet,
    BadNoisy,
    Done,
}

pub struct MovePicker {
    list: MoveList,
    tt_move: Move,
    stage: Stage,
    // next slot to fill in the range the current stage is draining
    cur: usize,
    good_end: usize,
    // skip_quiets: bool
}

impl MovePicker {
    pub fn new(tt_move: Move) -> MovePicker {
        MovePicker {
            list: MoveList::new(),
            tt_move,
            stage: Stage::TtMove,
            cur: 0,
            good_end: 0,
            // skip_quiets: false
        }
    }

    pub fn gen_move(&mut self, board: &Board, noisy_only: bool) {
        board.gen_move(&mut self.list, noisy_only);
        self.good_end = self.list.noisy_end();
    }

    pub fn score_quiet(&mut self, board: &Board, thread_data: &ThreadData, ply: usize, keys: [Option<ContKey>; CONT_LEN]) {
        let (killer1, killer2) = thread_data.killer.probe(ply);
        for i in self.list.quiet_start()..MAX_LIST_LENGTH {
            let mv = self.list.get(i);
            let score = if mv == killer1      { KILLER1_SCORE }
                             else if mv == killer2 { KILLER2_SCORE }
                             else { thread_data.quiet_history(board.stm(), board.piece_at(mv.from()), mv, &keys) };
            self.list.score(i, score)
        }
    }
    
    pub fn score_noisy(&mut self, board: &Board) {
        for i in 0..self.list.noisy_end() {
            let mv = self.list.get(i);

            let bonus = if mv.is_promotion() && mv.promo_piece() == PieceType::Queen {
                PieceType::Queen.value()
            } else {
                0
            };
            self.list.score(i, mvv_lva(mv, board) + bonus)
        }
    }

    pub fn next(&mut self, board: &Board) -> Option<Move> {
        loop {
            match self.stage {
                Stage::TtMove => {
                    self.stage = Stage::GoodNoisy;
                    self.cur = 0;
                    if !self.tt_move.is_null() && self.generated(self.tt_move) {
                        return Some(self.tt_move);
                    }
                }
                Stage::GoodNoisy => match self.select_best(self.good_end) {
                    Some(mv_index) => {
                        // see partitioning
                        let mv = self.list.get(mv_index);
                        self.list.swap(mv_index, self.cur);
                        if see(board, mv, 0) {
                            self.cur += 1;
                            if mv != self.tt_move { return Some(mv) }
                        } else {
                            self.good_end -= 1; 
                            self.list.swap(self.cur, self.good_end); 
                        }
                    },
                    None => {
                        self.stage = Stage::Quiet;
                        self.cur = self.list.quiet_start();
                    }
                },
                Stage::Quiet => match self.pick(MAX_LIST_LENGTH) {
                    Some(mv) => if mv != self.tt_move { return Some(mv) },
                    None => {
                        self.stage = Stage::BadNoisy;
                        self.cur = self.good_end;
                    }
                },
                Stage::BadNoisy => match self.pick(self.list.noisy_end()) {
                    Some(mv) => if mv != self.tt_move { return Some(mv) },
                    None => self.stage = Stage::Done,
                },
                Stage::Done => return None,
            }
        }
    }

    // Selection sort step: move the best scoring entry in [cur, end) to `cur` and consume it.
    fn pick(&mut self, end: usize) -> Option<Move> {
        if let Some(best) = self.select_best(end) {
            self.list.swap(best, self.cur);
            let mv = self.list.get(self.cur);
            self.cur += 1;
            Some(mv)
        } else {
            None
        }
    }

    fn select_best(&self, end: usize) -> Option<usize> {
        if self.cur >= end {
            return None;
        }

        let mut best = self.cur;
        let mut best_score = self.list.get_score(best);

        for i in (self.cur + 1)..end {
            let score = self.list.get_score(i);
            if score > best_score {
                best = i;
                best_score = score;
            }
        }

        Some(best)
    }

    fn generated(&self, mv: Move) -> bool {
        (0..self.list.len()).any(|i| self.list.get_nth(i) == mv)
    }

    pub fn terminal(&self) -> bool {
        self.list.len() == 0
    }

    pub fn random(&mut self, hash: u64) -> Move {
        let total = self.list.len();
        if total == 0 {
            return Move::default();
        }

        let mut z = hash | 1;
        z = z.wrapping_add(0x9E3779B97F4A7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^= z >> 31;

        self.list.get_nth((z % total as u64) as usize)
    }

    pub fn skip_quiets(&mut self) {
        debug_assert!(self.stage == Stage::Quiet);

        // self.skip_quiets = true;
        self.stage = Stage::BadNoisy;
        self.cur = self.good_end;
    }
    
    pub fn is_bad(&self) -> bool { 
        self.stage == Stage::BadNoisy
    }
}

// SEE move ordering
pub(crate) fn see(board: &Board, mv: Move, threshold: i32) -> bool {
    let balance = board.piece_at(mv.capture_square()).value() - threshold;
    if balance < 0 {
        return false;
    }

    let attacker = board.piece_at(mv.from()).value();
    if balance - attacker >= 0 {
        return true;
    }

    let mut occ = board.occ();
    occ.clear(mv.from());
    if mv.is_enpassant() {
        occ.clear(mv.capture_square());
    }

    balance - inner_see(board, mv.to(), !board.stm(), &mut occ, attacker) >= 0
}

fn inner_see(board: &Board, square: Square, stm: Color, occ: &mut Bitboard, occupier: i32) -> i32 {
    let attackers = board.attackers_to(square, *occ);

    let (piece_type, from) = 'lva: {
        for piece_type in PieceType::ALL {
            let bb = attackers & board.piece_bb(Piece::new(stm, piece_type));
            if !bb.is_empty() {
                break 'lva (piece_type, bb.lsb())
            }
        }
        return 0;
    };

    if piece_type == PieceType::King && !(attackers & board.color_bb(!stm)).is_empty() {
        return 0;
    }

    occ.clear(from);
    (occupier - inner_see(board, square, !stm, occ, piece_type.value())).max(0)
}


fn mvv_lva(mv: Move, board: &Board) -> i32 {
    board.piece_at(mv.capture_square()).value() - board.piece_at(mv.from()).value()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::MAX_LIST_LENGTH;

    const FENS: &[&str] = &[
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
        "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
        "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
        "n1n5/PPPk4/8/8/8/8/4Kppp/5N1N b - - 0 1",
        // in check: qsearch generates full evasions even when asked for noisy only
        "rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3",
    ];

    fn generated_moves(board: &Board, noisy_only: bool) -> Vec<u16> {
        let mut list = MoveList::new();
        board.gen_move(&mut list, noisy_only);
        let mut moves: Vec<u16> = (0..list.len()).map(|i| list.get_nth(i).raw()).collect();
        moves.sort();
        moves
    }

    fn drained_moves(board: &Board, noisy_only: bool, tt_move: Move) -> Vec<u16> {
        let td = ThreadData::new();
        let mut picker = MovePicker::new(tt_move);
        picker.gen_move(board, noisy_only);
        picker.score_quiet(board, &td, 0, [None; CONT_LEN]);

        let mut moves = Vec::new();
        while let Some(mv) = picker.next(board) {
            moves.push(mv.raw());
        }
        moves
    }

    // The whole risk of the double ended layout is a move getting lost in the SEE partition,
    // yielded twice, or the tt move being yielded again from its slot. Node counts cannot see
    // any of those; comparing the drained multiset against raw movegen can.
    #[test]
    fn picker_yields_every_generated_move_exactly_once() {
        for &fen in FENS {
            for noisy_only in [false, true] {
                let board = Board::from_fen(fen).expect(fen);
                let expected = generated_moves(&board, noisy_only);

                // no tt move
                let mut got = drained_moves(&board, noisy_only, Move::NULL);
                assert_eq!(got.len(), expected.len(), "count, no tt move, {fen}");
                got.sort();
                assert_eq!(got, expected, "moves, no tt move, {fen}");

                // every generated move in turn as the tt move: it must come out first, and
                // exactly once.
                for &raw in &expected {
                    let tt_move = Move::from_raw(raw);
                    let got_tt = drained_moves(&board, noisy_only, tt_move);
                    assert_eq!(got_tt[0], raw, "tt move not yielded first, {fen}");

                    let mut sorted = got_tt.clone();
                    sorted.sort();
                    assert_eq!(sorted, expected, "moves with tt move {tt_move}, {fen}");
                }

                // a move that was never generated must not be yielded
                let bogus = Move::new(Square::A1, Square::H8, crate::types::MoveKind::Normal);
                if !expected.contains(&bogus.raw()) {
                    let mut got_bogus = drained_moves(&board, noisy_only, bogus);
                    got_bogus.sort();
                    assert_eq!(got_bogus, expected, "bogus tt move leaked, {fen}");
                }
            }
        }
    }

    // The two regions must stay disjoint, and quiets must read back in the order they were
    // pushed (an increment-then-write bug in push_back drops the first quiet silently).
    #[test]
    fn list_regions_are_disjoint_and_complete() {
        for &fen in FENS {
            let board = Board::from_fen(fen).expect(fen);
            let mut list = MoveList::new();
            board.gen_move(&mut list, false);

            assert!(list.noisy_end() <= list.quiet_start(), "regions overlap, {fen}");
            assert_eq!(list.len(), list.noisy_len() + list.quiet_len(), "{fen}");
            assert_eq!(list.quiet_len(), MAX_LIST_LENGTH - list.quiet_start(), "{fen}");

            for i in 0..list.noisy_len() {
                assert!(list.get_nth(i).is_noisy(), "quiet move in noisy region, {fen}");
            }
            for i in list.noisy_len()..list.len() {
                assert!(!list.get_nth(i).is_noisy(), "noisy move in quiet region, {fen}");
            }
        }
    }
}
