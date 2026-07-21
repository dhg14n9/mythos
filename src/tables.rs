use crate::types::Move;

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


