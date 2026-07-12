use crate::board::board::Board;
use crate::types::{Move, MoveList};

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
        todo!()
    }
    pub fn score_noisy(&mut self) {
        todo!()
    }
    pub fn next(&mut self) -> Option<Move> {
        todo!()
    }

    pub fn terminal(&self) -> bool {
        (self.noisy.len() == 0) && (self.quiet.len() == 0)
    }

    pub fn random(&mut self, hash: u64) -> Move {
        let total = self.quiet.len() + self.noisy.len();
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
        } else {
            self.quiet.get(r - self.noisy.len())
        }
    }
}
