use crate::types::moves::Move;
use crate::types::uninit_array::UninitArray;

pub const MAX_LIST_LENGTH: usize = 256;

#[derive(Clone, Copy, Default)]
pub struct MoveEntry {
    score: i32,
    mv: Move,
}


//
//   0                            noisy_end        quiet_start                MAX_LIST_LENGTH
//   +------- noisy -------------------+---- gap ------+--------- quiets ------------+
//
// No legal position has more than 218 moves, so the two regions cannot meet.
#[derive(Clone)]
pub struct MoveList {
    array: UninitArray<MoveEntry, MAX_LIST_LENGTH>,
}

impl MoveList {
    pub fn new() -> Self {
        Self {
            array: UninitArray::new(),
        }
    }

    pub fn push_noisy(&mut self, mv: Move) {
        self.array.push(MoveEntry { mv, score: 0 });
    }

    pub fn push_quiet(&mut self, mv: Move) {
        self.array.push_back(MoveEntry { mv, score: 0 });
    }

    pub fn clear(&mut self) {
        self.array.clear();
    }

    // one past the last noisy move
    pub fn noisy_end(&self) -> usize {
        self.array.len()
    }

    // index of the first quiet move
    pub fn quiet_start(&self) -> usize {
        self.array.back()
    }

    pub fn noisy_len(&self) -> usize {
        self.array.len()
    }

    pub fn quiet_len(&self) -> usize {
        self.array.back_len()
    }

    pub fn len(&self) -> usize {
        self.noisy_len() + self.quiet_len()
    }

    pub fn get(&self, index: usize) -> Move {
        self.array.read(index).mv
    }

    pub fn get_score(&self, index: usize) -> i32 {
        self.array.read(index).score
    }

    pub fn score(&mut self, index: usize, score: i32) {
        self.array.read_mut(index).score = score;
    }

    pub fn swap(&mut self, i1: usize, i2: usize) {
        self.array.swap(i1, i2);
    }

    // The two regions are not contiguous, so callers that just want to walk every move (perft,
    // UCI move lookup) index by position in 0..len() instead of by slot.
    pub fn get_nth(&self, n: usize) -> Move {
        debug_assert!(n < self.len());

        if n < self.noisy_len() {
            self.get(n)
        } else {
            self.get(self.quiet_start() + n - self.noisy_len())
        }
    }
}
