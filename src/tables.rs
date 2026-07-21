use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use crate::types::{Color, Move, Square};

// trans table
#[derive(Default, Copy, Clone)]
#[repr(u8)]
pub enum BoundType {
    #[default]
    Exact = 0,
    Lower,
    Upper
}

#[derive(Default)]
pub struct Slot {
    key: AtomicU64,
    data: AtomicU64
}

pub struct TransTable {
    array: Arc<[Slot]>,
    num_entry: usize
}

impl TransTable {
    pub fn new(size_mb: usize) -> Self {
        let num_entry = (size_mb.max(1) * 1024 * 1024) / size_of::<Slot>();
        let array: Arc<[Slot]> = (0..num_entry).map(|_| Slot::default()).collect();
        Self { array, num_entry }
    }
    fn index(key: u64, num_entry: usize) -> usize {
        ((key as u128 * num_entry as u128) >> 64) as usize
    }

    pub fn probe(&self, key: u64) -> Option<(i32, Move, usize, BoundType)> {
        let slot = &self.array[Self::index(key, self.num_entry)];
        let key_cell = slot.key.load(Ordering::Relaxed);
        let data = slot.data.load(Ordering::Relaxed);
        if key_cell ^ data == key {
            Some(Self::unpack(data))
        } else {
            None
        }
    }

    pub fn store(&self, key: u64, score: i32, best: Move, depth: usize, bound_type: BoundType) {
        let slot = &self.array[Self::index(key, self.num_entry)];
        let data = Self::pack(score, best, depth, bound_type);
        slot.key.store(key ^ data, Ordering::Relaxed);
        slot.data.store(data, Ordering::Relaxed);
    }

    pub fn clear(&self) {
        for slot in self.array.iter() {
            slot.key.store(0, Ordering::Relaxed);
            slot.data.store(0, Ordering::Relaxed);
        }
    }

    // pack score, move, depth, bound into a u64.
    // score must be on top because when score as u64, every bit on the right will be corrupted into
    // ones. score on top will throw these (ones) on top as redundant bits
    fn pack(score: i32, best: Move, depth: usize, bound_type: BoundType) -> u64 {
        ((score as u64) << 26) | ((best.raw() as u64) << 10) | ((depth as u64) << 2) | bound_type as u64
    }

    fn unpack(data: u64) -> (i32, Move, usize, BoundType) {
        let score = (data >> 26) as i32;
        let mv = Move::from_raw(((data >> 10) & 0xffff) as u16);
        let depth = ((data >> 2) & 0xff) as usize;
        let bound_type = match data & 3 {
            0 => BoundType::Exact,
            1 => BoundType::Lower,
            _ => BoundType::Upper
        };
        (score, mv, depth, bound_type)
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

pub struct ThreadData {
    pub history: History,
    pub killer: Killer,
}

impl ThreadData {
    pub fn new() -> Self {
        Self {
            history: History::new(),
            killer: Killer::new()
        }
    }

    pub fn clear(&mut self) {
        *self = Self::new();
    }

}
