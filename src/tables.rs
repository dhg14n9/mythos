use std::sync::Arc;
use std::sync::atomic::{AtomicI16, AtomicU16, AtomicU8};
use std::sync::atomic::Ordering::Relaxed;
use crate::types::{Color, Move, Piece, PieceType, Score, Square};

#[derive(Default, Copy, Clone, PartialEq, Debug)]
#[repr(u8)]
pub enum BoundType {
    #[default]
    Exact = 0,
    Lower,
    Upper
}

pub struct TTHit {
    pub score: i32,
    pub mv: Move,
    pub depth: usize,
    pub bound: BoundType,
    pub eval: i32
}

#[derive(Default)]
#[repr(C)]
struct Entry {
    key: AtomicU16,
    mv: AtomicU16,
    score: AtomicI16,
    eval: AtomicI16,
    depth: AtomicU8,
    flags: AtomicU8
}

const CLUSTER_SIZE: usize = 3;

#[derive(Default)]
#[repr(C, align(32))]
struct Cluster {
    entries: [Entry; CLUSTER_SIZE]
}

const _: () = assert!(size_of::<Entry>() == 10);
const _: () = assert!(size_of::<Cluster>() == 32);

#[derive(Clone)]
pub struct TransTable {
    array: Arc<[Cluster]>,
    num_cluster: usize,
    pub generation: u8,
}

// flags: [ age: 5 ][ pv: 1, unused ][ bound: 2 ]
const AGE_SHIFT: u8 = 3;
const BOUND_MASK: u8 = 3;

// Stored depth is offset by one so that 0 marks an empty entry.
const DEPTH_OFFSET: usize = 1;

const AGE_PEN: i32 = 4;


impl TransTable {
    pub const AGE_MASK: u8 = 0x1F;

    pub fn new(size_mb: usize) -> Self {
        let num_cluster = (size_mb.max(1) * 1024 * 1024) / size_of::<Cluster>();
        let array: Arc<[Cluster]> = (0..num_cluster).map(|_| Cluster::default()).collect();
        Self { array, num_cluster, generation: 0 }
    }

    fn index(key: u64, num_cluster: usize) -> usize {
        ((key as u128 * num_cluster as u128) >> 64) as usize
    }

    fn cluster(&self, key: u64) -> &Cluster {
        &self.array[Self::index(key, self.num_cluster)]
    }

    pub fn prefetch(&self, key: u64) {
        #[cfg(target_arch = "x86_64")]
        unsafe {
            use std::arch::x86_64::{_mm_prefetch, _MM_HINT_T0};
            _mm_prefetch::<_MM_HINT_T0>(self.cluster(key) as *const Cluster as *const i8);
        }
        #[cfg(not(target_arch = "x86_64"))]
        let _ = key;
    }

    pub fn probe(&self, key: u64) -> Option<TTHit> {
        let key16 = key as u16;
        let entry = self.cluster(key).entries.iter()
            .find(|e| e.key.load(Relaxed) == key16 && e.depth.load(Relaxed) != 0)?;

        let score = entry.score.load(Relaxed) as i32;
        let eval = entry.eval.load(Relaxed) as i32;
        let mv = Move::from_raw(entry.mv.load(Relaxed));
        let depth = entry.depth.load(Relaxed) as usize - DEPTH_OFFSET;
        let bound = match entry.flags.load(Relaxed) & BOUND_MASK {
            0 => BoundType::Exact,
            1 => BoundType::Lower,
            _ => BoundType::Upper
        };
        Some(TTHit { score, mv, depth, bound, eval })
    }

    pub fn store(&self, key: u64, score: i32, best: Move, depth: usize, bound_type: BoundType, eval: i32) {
        debug_assert!(score.abs() <= Score::NONE, "score {score} overflows the 16-bit field");
        debug_assert!(depth + DEPTH_OFFSET < 256, "depth {depth} overflows the 8-bit field");

        let key16 = key as u16;
        let entries = &self.cluster(key).entries;

        let entry = entries.iter()
            .find(|e| e.depth.load(Relaxed) == 0 || e.key.load(Relaxed) == key16)
            .unwrap_or_else(|| entries.iter().min_by_key(|e| self.quality(e)).unwrap());

        let same_key = entry.key.load(Relaxed) == key16 && entry.depth.load(Relaxed) != 0;

        if !(same_key && best.is_null()) {
            entry.mv.store(best.raw(), Relaxed);
        }

        if same_key
            && bound_type != BoundType::Exact
            && self.age(entry) == 0
            && depth + DEPTH_OFFSET + 4 <= entry.depth.load(Relaxed) as usize
        {
            return;
        }

        entry.key.store(key16, Relaxed);
        entry.score.store(score as i16, Relaxed);
        entry.eval.store(eval as i16, Relaxed);
        entry.depth.store((depth + DEPTH_OFFSET) as u8, Relaxed);
        entry.flags.store((self.generation << AGE_SHIFT) | bound_type as u8, Relaxed);
    }

    pub fn clear(&self) {
        for entry in self.array.iter().flat_map(|c| &c.entries) {
            entry.key.store(0, Relaxed);
            entry.mv.store(0, Relaxed);
            entry.score.store(0, Relaxed);
            entry.eval.store(0, Relaxed);
            entry.depth.store(0, Relaxed);
            entry.flags.store(0, Relaxed);
        }
    }

    pub fn hashfull(&self) -> usize {
        let sample = self.num_cluster.min(1000);
        if sample == 0 {
            return 0;
        }
        let used = self.array[..sample]
            .iter()
            .flat_map(|c| &c.entries)
            .filter(|e| e.depth.load(Relaxed) != 0 && self.age(e) == 0)
            .count();
        used * 1000 / (sample * CLUSTER_SIZE)
    }

    fn age(&self, entry: &Entry) -> u8 {
        let entry_age = entry.flags.load(Relaxed) >> AGE_SHIFT;
        self.generation.wrapping_sub(entry_age) & Self::AGE_MASK
    }

    fn quality(&self, entry: &Entry) -> i32 {
        entry.depth.load(Relaxed) as i32 - AGE_PEN * self.age(entry) as i32
    }

}

pub const MAX_PLY: usize = 256;

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

    pub fn probe(&self, ply: usize) -> (Move, Move) {
        self.array[ply].into()
    }

}

const MAX_BUTTERFLY: i32 = 8192;

fn apply<const MAX: i32>(entry: &mut i32, bonus: i32) {
    *entry += bonus - *entry * bonus.abs() / MAX
}

pub struct Butterfly {
    array: Box<[[[i32; 64]; 64]; 2]>
}

impl Butterfly {
    pub fn new() -> Self {
        Self {
            array: Box::from([[[0; 64]; 64]; 2])
        }
    }
    pub fn probe(&self, color: Color, from: Square, to: Square) -> i32 {
        self.array[color][from][to]
    }

    pub fn update(&mut self, color: Color, from: Square, to: Square, bonus: i32) {
        apply::<MAX_BUTTERFLY>(&mut self.array[color][from][to], bonus)
    }

}

const MAX_CONTINUATION: i32 = 15000;

pub const CONT_OFFSET: [usize; 4] = [1, 2, 4, 6];
pub const CONT_LEN: usize = CONT_OFFSET.len();
pub const CONT_READ: usize = 2;


pub struct Continuation {
    array: Box<[[[[i16; Square::NUM]; Piece::NUM]; Square::NUM]; Piece::NUM]>
}

impl Continuation {
    pub fn new() -> Self {
        Self {
            array: Box::try_from(vec![[[[0; Square::NUM]; Piece::NUM]; Square::NUM]; Piece::NUM].into_boxed_slice()).unwrap()
        }
    }

    pub fn probe(&self, prev_piece: Piece, prev_to: Square, piece: Piece, to: Square) -> i32 {
        self.array[prev_piece][prev_to][piece][to] as i32
    }

    pub fn update(&mut self, prev_piece: Piece, prev_to: Square, piece: Piece, to: Square, bonus: i32) {
        let entry = &mut self.array[prev_piece][prev_to][piece][to];
        let mut value = *entry as i32;
        apply::<MAX_CONTINUATION>(&mut value, bonus);
        *entry = value as i16
    }

}

const MAX_CAPTURE: i32 = 16384;

pub struct Capture {
    array: Box<[[[i16; 7]; Square::NUM]; Piece::NUM]>
}

impl Capture {
    pub fn new() -> Self {
        Self {
            array: Box::try_from(vec![[[0; 7]; Square::NUM]; Piece::NUM].into_boxed_slice()).unwrap()
        }
    }

    pub fn probe(&self, piece: Piece, to_square: Square, captured: PieceType) -> i32 {
        self.array[piece][to_square][captured] as i32
    }

    pub fn update(&mut self, piece: Piece, to_square: Square, captured: PieceType, bonus: i32) {
        let entry = &mut self.array[piece][to_square][captured];
        let mut value = *entry as i32;
        apply::<MAX_CAPTURE>(&mut value, bonus);
        *entry = value as i16;
    }
}

#[derive(Copy, Clone)]
pub struct ContKey {
    pub(crate) piece: Piece,
    pub(crate) square: Square
}

const CORR_SIZE: usize = 16384;
pub const MAX_CORR: i32 = 16384;

pub struct Correction {
    array: Box<[[i16; CORR_SIZE]; 2]>
}

impl Correction {
    pub fn new() -> Self {
        Self {
            array: Box::from([[0; CORR_SIZE]; 2])
        }
    }

    pub fn probe(&self, color: Color, pawn_key: u64) -> i32 {
        self.array[color][pawn_key as usize & (CORR_SIZE - 1)] as i32
    }

    pub fn update(&mut self, color: Color, pawn_key: u64, bonus: i32) {
        let entry = &mut self.array[color][pawn_key as usize & (CORR_SIZE - 1)];
        let mut value = *entry as i32;
        apply::<MAX_CORR>(&mut value, bonus);
        *entry = value as i16
    }
}

pub struct ThreadData {
    pub butterfly: Butterfly,
    pub killer: Killer,
    pub continuation: Continuation,
    pub capture: Capture,
    pub correction: Correction
}

impl ThreadData {
    pub fn new() -> Self {
        Self {
            butterfly: Butterfly::new(),
            killer: Killer::new(),
            continuation: Continuation::new(),
            capture: Capture::new(),
            correction: Correction::new()
        }
    }


    pub fn quiet_history(&self, color: Color, moved: Piece, mv: Move, keys: &[Option<ContKey>; CONT_LEN]) -> i32 {
        let mut score = self.butterfly.probe(color, mv.from(), mv.to()) * 2;
        for i in 0..CONT_READ {
            if let Some(key) = keys[i] {
                score += self.continuation.probe(key.piece, key.square, moved, mv.to());
            }
        }
        score
    }

    pub fn clear(&mut self) {
        *self = Self::new();
    }

}
