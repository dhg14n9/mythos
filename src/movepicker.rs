use crate::board::board::Board;
use crate::types::{Bitboard, Color, Move, MoveList, Piece, PieceType, Square};

pub struct MovePicker {
    quiet: MoveList,
    noisy: MoveList,
    bad_noisy: MoveList,
}

impl MovePicker {
    pub fn new() -> MovePicker {
        MovePicker {
            quiet: MoveList::new(),
            noisy: MoveList::new(),
            bad_noisy: MoveList::new(),
        }
    }

    pub fn gen_move(&mut self, board: &Board) {
        board.gen_move(&mut self.quiet, &mut self.noisy)
    }

    pub fn score_quiet(&mut self) {

    }
    pub fn score_noisy(&mut self, board: &Board) {
        for i in 0..self.noisy.len() {
            if !see(board, self.noisy.get(i), 0) {
                self.bad_noisy.push(self.noisy.remove(i));
            }
        }


    }
    pub fn next(&mut self) -> Option<Move> {
        if let Some(mv) = self.noisy.next() {
            return Some(mv);
        } else if let Some(mv) = self.bad_noisy.next() {
            return Some(mv);
        }
        self.quiet.next()
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

