use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::atomic::Ordering::Relaxed;
use crate::types::{Color, Move, Piece, Square};

// trans table
#[derive(Default, Copy, Clone, PartialEq, Debug)]
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

#[derive(Clone)]
pub struct TransTable {
    array: Arc<[Slot]>,
    num_entry: usize,
    pub generation: u8,
}

const BOUND_SHIFT: usize =  0;
const DEPTH_SHIFT: usize =  2;
const MOVE_SHIFT : usize = 10;
const AGE_SHIFT  : usize = 26;
const SCORE_SHIFT: usize = 46;

const AGE_PEN: u8 = 4;


impl TransTable {
    pub const AGE_MASK: u8 = 0x3F;

    pub fn new(size_mb: usize) -> Self {
        let num_entry = (size_mb.max(1) * 1024 * 1024) / size_of::<Slot>();
        let array: Arc<[Slot]> = (0..num_entry).map(|_| Slot::default()).collect();
        Self { array, num_entry, generation: 0 }
    }
    fn index(key: u64, num_entry: usize) -> usize {
        ((key as u128 * num_entry as u128) >> 64) as usize
    }

    pub fn probe(&self, key: u64) -> Option<(i32, Move, usize, BoundType)> {
        let slot = &self.array[Self::index(key, self.num_entry)];
        let key_cell = slot.key.load(Ordering::Relaxed);
        let data = slot.data.load(Ordering::Relaxed);
        if key_cell ^ data == key {
            let (score, mv, depth, bound_type, _) = Self::unpack(data);
            Some((score, mv, depth, bound_type))
        } else {
            None
        }
    }

    pub fn store(&self, key: u64, score: i32, best: Move, depth: usize, bound_type: BoundType) {
        let slot = &self.array[Self::index(key, self.num_entry)];
        let (_, _, entry_depth, _, entry_age) = Self::unpack(slot.data.load(Relaxed));
        let real_age = (self.generation.wrapping_sub(entry_age)) & Self::AGE_MASK;

        if (depth as i32) < Self::quality(entry_depth, real_age) {
            return
        }

        let data = Self::pack(score, best, depth, bound_type, self.generation);
        slot.key.store(key ^ data, Ordering::Relaxed);
        slot.data.store(data, Ordering::Relaxed);
    }

    pub fn clear(&self) {
        for slot in self.array.iter() {
            slot.key.store(0, Ordering::Relaxed);
            slot.data.store(0, Ordering::Relaxed);
        }
    }


    pub fn hashfull(&self) -> usize {
        let sample = self.num_entry.min(1000);
        if sample == 0 {
            return 0;
        }
        let used = self.array[..sample]
            .iter()
            .filter(|slot| {
                slot.key.load(Ordering::Relaxed) != 0 || slot.data.load(Ordering::Relaxed) != 0
            })
            .count();
        used * 1000 / sample
    }

    // The whole entry lives in one u64:
    //
    //   63            46 45      32 31       26 25        10 9       2 1   0
    //  [ score: 18 sgn ][ free: 14 ][ age: 6  ] [ move: 16 ] [depth: 8] [bnd]
    //
    // score sits at the top so unpack recovers its sign with a single arithmetic
    // shift (`data as i64 >> SCORE_SHIFT`) rather than masking and sign-extending
    // by hand -- a logical shift here silently turns every negative score into a
    // large positive one. age is the generation that wrote the entry, 6 bits so it
    // wraps every 64 searches (hence the wrapping_sub in store). Bits 32..45 are
    // free -- wide enough for a static eval, the usual next tenant.
    fn pack(score: i32, best: Move, depth: usize, bound_type: BoundType, age: u8) -> u64 {
        debug_assert!(
            (-(1 << 17)..(1 << 17)).contains(&score),
            "score {score} overflows the 18-bit field"
        );
        debug_assert!(depth < 256, "depth {depth} overflows the 8-bit field");

        ((score as u64) << SCORE_SHIFT)         |
            ((best.raw() as u64) << MOVE_SHIFT) |
            ((depth as u64) << DEPTH_SHIFT)     |
            ((bound_type as u64) << BOUND_SHIFT)|
            ((age as u64) << AGE_SHIFT)
    }

    fn unpack(data: u64) -> (i32, Move, usize, BoundType, u8) {
        let score = (data as i64 >> SCORE_SHIFT) as i32;
        let mv = Move::from_raw(((data >> MOVE_SHIFT) & 0xffff) as u16);
        let depth = ((data >> DEPTH_SHIFT) & 0xff) as usize;
        let bound_type = match data & 3 {
            0 => BoundType::Exact,
            1 => BoundType::Lower,
            _ => BoundType::Upper
        };
        let age = ((data >> AGE_SHIFT) as u8) & Self::AGE_MASK;
        (score, mv, depth, bound_type, age)
    }

    fn quality(depth: usize, real_age: u8) -> i32 {
        depth as i32 - (AGE_PEN as i32 * real_age as i32)
    }

}

// killer heuristics
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

    // return NULL if there isn't a
    pub fn probe(&self, ply: usize) -> (Move, Move) {
        self.array[ply].into()
    }

}

// Butterfly history heuristic
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

#[derive(Copy, Clone)]
pub struct ContKey {
    pub(crate) piece: Piece,
    pub(crate) square: Square
}

pub struct ThreadData {
    pub butterfly: Butterfly,
    pub killer: Killer,
    pub continuation: Continuation
}

impl ThreadData {
    pub fn new() -> Self {
        Self {
            butterfly: Butterfly::new(),
            killer: Killer::new(),
            continuation: Continuation::new(),
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
