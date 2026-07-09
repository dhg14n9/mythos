use crate::types::moves::Move;
use crate::types::uninit_array::UninitArray;

pub const MAX_LIST_LENGTH: usize = 256;

#[derive(Clone, Copy, Default)]
pub struct MoveEntry {
    score: i32,
    mv: Move,
}

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

    pub fn push(&mut self, mv: Move) {
        self.array.push(MoveEntry { mv, score: 0 });
    }

    pub fn clear(&mut self) {
        self.array.clear();
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

    fn swap(&mut self, i1: usize, i2: usize) {
        self.array.swap(i1, i2);
    }

    pub fn next(&mut self, n: usize) -> Move {
        let mut best = n;
        let mut best_score = self.get_score(n);

        for i in (n + 1)..self.array.len() {
            if self.get_score(i) > best_score {
                best = i;
                best_score = self.get_score(i);
            }
        }
        self.swap(n, best);
        self.get(n)
    }

    pub fn len(&self) -> usize {
        self.array.len()
    }
}
