use std::mem::MaybeUninit;
use crate::types::moves::Move;

// a chess position have maximum about 213 legal move
pub const MAX_LIST_LENGTH: usize = 256;

#[derive(Clone, Copy, Default)]
pub struct MoveEntry {
    score: i32,
    mv: Move,
}

#[derive(Clone)]
pub struct MoveList {
    array: [MaybeUninit<MoveEntry>; MAX_LIST_LENGTH],
    length: usize
}

impl MoveList {
    pub fn new() -> Self {
        Self {
            array: [MaybeUninit::uninit(); MAX_LIST_LENGTH],
            length: 0
        }
    }

    pub fn push(&mut self, mv: Move) {
        debug_assert!(self.length < MAX_LIST_LENGTH); // theoretically not possible but just for safety

        self.array[self.length].write(MoveEntry { mv, score: 0 });
        self.length += 1;
    }
    pub fn clear(&mut self) {
        self.length = 0;
    }
    pub fn get(&self, index: usize) -> Move {
        debug_assert!(index < self.length);

        unsafe { self.array[index].assume_init().mv }
    }
    pub fn get_score(&self, index: usize) -> i32 {
        debug_assert!(index < self.length);

        unsafe { self.array[index].assume_init().score }
    }

    pub fn score(&mut self, index: usize, score: i32) {
        debug_assert!(index < self.length);

        unsafe { self.array[index].assume_init_mut().score = score }
    }

    fn swap(&mut self, i1: usize, i2: usize) {
        debug_assert!(i1 < self.length && i2 < self.length);

        let temp = self.array[i1];
        self.array[i1] = self.array[i2];
        self.array[i2] = temp
    }

    pub fn next(&mut self, n: usize) -> Move {
        let mut best = n;
        let mut best_score = self.get_score(n);

        for i in (n + 1)..self.length {
            if self.get_score(i) > best_score {
                best = i;
                best_score = self.get_score(i);

            }
        }
        self.swap(n, best);
        self.get(n)
    }
}