use crate::board::board::Board;
use crate::tables::ThreadData;
use crate::types::{Bitboard, Color, Move, MoveList, Piece, PieceType, Square};

const KILLER1_SCORE: i32 = 1_000_000;
const KILLER2_SCORE: i32 = 900_000;

pub struct MovePicker {
    quiet: MoveList,
    noisy: MoveList,
    bad_noisy: MoveList,
    tt_move: Move,
    tt_yielded: bool
}

impl MovePicker {
    pub fn new(tt_move: Move) -> MovePicker {
        MovePicker {
            quiet: MoveList::new(),
            noisy: MoveList::new(),
            bad_noisy: MoveList::new(),
            tt_move,
            tt_yielded: false
        }
    }

    pub fn gen_move(&mut self, board: &Board, noisy_only: bool) {
        board.gen_move(&mut self.quiet, &mut self.noisy, noisy_only)
    }

    pub fn score_quiet(&mut self, thread_data: &ThreadData, stm: Color, ply: usize) {
        let (killer1, killer2) = thread_data.killer.probe(ply);
        for i in 0..self.quiet.len() {
            let mv = self.quiet.get(i);
            let score = if mv == killer1      { KILLER1_SCORE }
                             else if mv == killer2 { KILLER2_SCORE }
                             else { thread_data.history.probe(stm, mv.from(), mv.to()) };
            self.quiet.score(i, score)
        }
    }
    pub fn score_noisy(&mut self, board: &Board) {
        let mut i = 0;
        while i < self.noisy.len() {
            if !see(board, self.noisy.get(i), 0) {
                self.bad_noisy.push(self.noisy.remove(i));
            } else {
                i += 1;
            }
        }

    }
    pub fn next(&mut self) -> Option<Move> {
        // tt move first
        if !self.tt_yielded {
            self.tt_yielded = true;
            if !self.tt_move.is_null() && self.generated(self.tt_move) {
                return Some(self.tt_move);
            }
        }
        
        while let Some(mv) = self.noisy.next() {
            if mv != self.tt_move {
                return Some(mv);
            }
        }
        while let Some(mv) = self.quiet.next() {
            if mv != self.tt_move {
                return Some(mv);
            }
        }
        while let Some(mv) = self.bad_noisy.next() {
            if mv != self.tt_move {
                return Some(mv);
            }
        }
        None
    }

    fn generated(&self, mv: Move) -> bool {
        let contains = |list: &MoveList| (0..list.len()).any(|i| list.get(i) == mv);
        contains(&self.noisy) || contains(&self.quiet) || contains(&self.bad_noisy)
    }

    pub fn terminal(&self) -> bool {
        (self.noisy.len() == 0) && (self.quiet.len() == 0) && (self.bad_noisy.len() == 0)
    }

    pub fn random(&mut self, hash: u64) -> Move {
        let total = self.quiet.len() + self.noisy.len() + self.bad_noisy.len();
        if total == 0 {
            return Move::default();
        }

        let mut z = hash | 1;
        z = z.wrapping_add(0x9E3779B97F4A7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^= z >> 31;

        let r = (z % total as u64) as usize;
        if r < self.noisy.len() {
            self.noisy.get(r)
        } else if r < self.noisy.len() + self.bad_noisy.len() {
            self.bad_noisy.get(r - self.noisy.len())
        } else {
            self.quiet.get(r - self.noisy.len() - self.bad_noisy.len())
        }
    }
}

// SEE move ordering
fn see(board: &Board, mv: Move, threshold: i32) -> bool {
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

