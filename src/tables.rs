use crate::types::{Color, Move, Square};

// trans table
#[derive(Default, Copy, Clone)]
pub enum BoundType {
    #[default]
    Exact,
    Lower,
    Upper
}

#[derive(Default, Copy, Clone)]
pub struct TTEntry {
    key: u64,
    score: i32,
    best: Move,
    depth: u8,
    bound_type: BoundType
}

pub struct TransTable {
    array: Vec<TTEntry>,
    num_entry: usize
}

impl TransTable {
    pub fn new(size_mb: usize) -> Self {
        let num_entry = (size_mb.max(1) * 1024 * 1024) / size_of::<TTEntry>();
        Self {
            array: vec![TTEntry::default(); num_entry],
            num_entry
        }
    }
    fn index( key: u64, num_entry: usize) -> usize {
        ((key as u128 * num_entry as u128) >> 64) as usize
    }

    pub fn probe(&self, key: u64) -> Option<(i32, Move, usize, BoundType)> {
        let entry = self.array[Self::index(key, self.num_entry)];
        if entry.key == key {
            Some((entry.score, entry.best, entry.depth as usize, entry.bound_type))
        } else {
            None
        }
    }

    pub fn store(&mut self, key: u64, score: i32, best: Move, depth: usize, bound_type: BoundType) {
        self.array[Self::index(key, self.num_entry)] = TTEntry {
            key, score, best, depth: depth as u8, bound_type
        };
    }
}

// killer heuristics
const MAX_PLY: usize = 256;

pub struct Killer {
    array: Box<[[Move; 2]; MAX_PLY]>
}

impl Killer {
    pub fn new() -> Self {
        Self {
            array: Box::from([[Move::NULL; 2]; MAX_PLY])
        }
    }

    pub fn store(&mut self, mv: Move, ply: usize) {
        if self.array[ply][0] != mv {
            self.array[ply][1] = self.array[ply][0];
            self.array[ply][0] = mv;
        }
    }

    // return NULL if there isn't a
    pub fn probe(&self, ply: usize) -> (Move, Move) {
        self.array[ply].into()
    }

}

// History heuristic
const MAX: i32 = 8192;
pub struct History {
    array: Box<[[[i32; 64]; 64]; 2]>
}

impl History {
    pub fn new() -> Self {
        Self {
            array: Box::from([[[0; 64]; 64]; 2])
        }
    }
    pub fn probe(&self, color: Color, from: Square, to: Square) -> i32 {
        self.array[color][from][to]
    }

    fn apply_bonus(&mut self, color: Color, from: Square, to: Square, bonus: i32) {
        self.array[color][from][to] += bonus - self.array[color][from][to] * bonus.abs() / MAX
    }

    pub fn bonus(&mut self, color: Color, from: Square, to: Square, depth: usize) {
        let bonus = (depth * depth).min(1200) as i32;
        self.apply_bonus(color, from, to, bonus)
    }

    pub fn malus(&mut self, color: Color, from: Square, to: Square, depth: usize) {
        let malus = -((depth * depth).min(1200) as i32);
        self.apply_bonus(color, from, to, malus)
    }

}

