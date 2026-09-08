use std::ops::{Add, AddAssign, Sub, SubAssign};
use crate::board::board::Board;
use crate::nnue::{BUCKET_COUNT, BUCKET_SIZE, HL, KING_LAYOUT};
use crate::nnue::network::Network;
use crate::types::{Bitboard, Color, Move, Piece, PieceType, Square};

#[repr(C, align(64))]
#[derive(Copy, Clone, PartialEq)]
pub struct Accumulator([i16; HL]);

impl Accumulator {
    pub fn empty() -> Self {
        Self([0; HL])
    }

    pub fn get(&self, index: usize) -> i16 {
        debug_assert!(index < HL);

        self.0[index]
    }

    pub fn set(&mut self, index: usize, x: i16) {
        self.0[index] = x
    }

    pub fn as_slice(&self) -> &[i16; HL] {
        &self.0
    }

    #[inline]
    pub fn set_add_sub(&mut self, parent: &Self, a0: &Self, s0: &Self) {
        for i in 0..HL {
            self.0[i] = parent.0[i] + a0.0[i] - s0.0[i];
        }
    }

    #[inline]
    pub fn set_add_sub2(&mut self, parent: &Self, a0: &Self, s0: &Self, s1: &Self) {
        for i in 0..HL {
            self.0[i] = parent.0[i] + a0.0[i] - s0.0[i] - s1.0[i];
        }
    }

    #[inline]
    pub fn set_add2_sub2(&mut self, parent: &Self, a0: &Self, a1: &Self, s0: &Self, s1: &Self) {
        for i in 0..HL {
            self.0[i] = parent.0[i] + a0.0[i] + a1.0[i] - s0.0[i] - s1.0[i];
        }
    }
}

impl Add for Accumulator {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        let mut output = Self::empty();
        for i in 0..HL {
            output.0[i] = self.0[i] + rhs.0[i]
        }
        output
    }
}

impl AddAssign for Accumulator {
    fn add_assign(&mut self, rhs: Self) {
        for i in 0..HL {
            self.0[i] += rhs.0[i]
        }
    }
}

impl Sub for Accumulator {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        let mut output = Self::empty();
        for i in 0..HL {
            output.0[i] = self.0[i] - rhs.0[i]
        }
        output
    }
}

impl SubAssign for Accumulator {
    fn sub_assign(&mut self, rhs: Self) {
        for i in 0..HL {
            self.0[i] -= rhs.0[i]
        }
    }
}

#[derive(Copy, Clone)]
pub struct AccState {
    pub accs: [Accumulator; 2],
    pub computed: [bool; 2],
    pub mirror: [bool; 2],
    pub bucket: [usize; 2],
    pub delta: Delta
}

impl AccState {
    pub fn empty() -> Self {
        Self {
            accs: [Accumulator::empty(); 2],
            computed: [false; 2],
            mirror: [false; 2],
            bucket: [0; 2],
            delta: Delta::empty()
        }
    }
}

pub fn feature_index(perspective: Color, piece: Piece, square: Square, mirror: bool, bucket: usize) -> usize {
    let square = if mirror { square.flip_file() } else { square };

    (piece.piece_type() as usize) * 64 +
        (square.relative_to(perspective) as usize) +
        if perspective == piece.color() { 0 } else { 384 } +
        bucket * BUCKET_SIZE
}


pub fn king_context(board: &Board, color: Color) -> (bool, usize) {
    let king = board.piece_bb(Piece::new(color, PieceType::King)).lsb();
    let mirror = king.is_kingside();

    (mirror, king_bucket(color, king, mirror))
}

pub fn should_mirror(board: &Board, color: Color) -> bool {
    board.piece_bb(Piece::new(color, PieceType::King)).lsb().is_kingside()
}

pub fn king_bucket(perspective: Color, king: Square, mirror: bool) -> usize {
    let king = king.relative_to(perspective);
    let king = if mirror { king.flip_file() } else { king };
    let index = (king.rank() as usize * 4) + king.file() as usize;

    KING_LAYOUT[index]
}

// `mirror` and `bucket` describe the position *after* the move. A deferred
// entry is replayed against its parent's weight block, so the king may only
// stay deferred while both are unchanged -- a bucket change rewrites every
// feature index just as surely as a mirror flip does.
//
// The old bucket has to be read with the OLD square's own mirror flag, not the
// new one: `mirror` belongs to the square the king landed on, and folding the
// square it came from with it names a bucket that never existed.
pub fn needs_refresh(delta: &Delta, color: Color, mirror: bool, bucket: usize) -> bool {
    let king = Piece::new(color, PieceType::King);

    delta.subs().iter().any(|&(piece, square)| {
        if piece != king {
            return false;
        }

        let old_mirror = square.is_kingside();

        old_mirror != mirror || king_bucket(color, square, old_mirror) != bucket
    })
}

#[derive(Copy, Clone)]
pub struct Delta {
    adds: [(Piece, Square); 2],
    subs: [(Piece, Square); 2],
    num_add: usize,
    num_sub: usize,
}

impl Delta {

    pub fn empty() -> Self {
        Self {
            adds: [(Piece::None, Square::None); 2],
            subs: [(Piece::None, Square::None); 2],
            num_add: 0,
            num_sub: 0
        }
    }

    pub fn new(board: &Board, mv: Move) -> Self {
        let mv_piece = board.piece_at(mv.from());

        let mut adds = [(Piece::None, Square::None); 2];
        let mut subs = [(Piece::None, Square::None); 2];
        let mut num_add = 0;
        let mut num_sub = 0;

        if mv.is_capture() {
            let cap_square = mv.capture_square();
            subs[num_sub] = (board.piece_at(cap_square), cap_square);
            num_sub += 1;
        }

        if mv.is_promotion() {
            adds[num_add] = (Piece::new(board.stm(), mv.promo_piece()), mv.to());
            subs[num_sub] = (mv_piece, mv.from());
        } else {
            adds[num_add] = (mv_piece, mv.to());
            subs[num_sub] = (mv_piece, mv.from());
        }
        num_sub += 1;
        num_add += 1;

        if mv.is_castling() {
            let (rook_from, rook_to) = Board::castle_rook_squares(mv.kind(), mv.to());
            let rook = Piece::new(board.stm(), PieceType::Rook);
            adds[num_add] = (rook, rook_to);
            subs[num_sub] = (rook, rook_from);
            num_sub += 1;
            num_add += 1;
        }

        Self {
            adds, subs, num_add, num_sub
        }
    }

    pub fn adds(&self) -> &[(Piece, Square)] {
        &self.adds[..self.num_add]
    }

    pub fn subs(&self) -> &[(Piece, Square)] {
        &self.subs[..self.num_sub]
    }
}


pub struct FinnyEntry {
    acc: Accumulator,
    bb: [Bitboard; Piece::NUM],
}

impl FinnyEntry {
    pub fn new(net: &Network) -> Self {
        Self {
            acc: net.feature_bias(),
            bb: [Bitboard::EMPTY; Piece::NUM]
        }
    }
}

pub struct FinnyTable([[[FinnyEntry; BUCKET_COUNT]; 2 /*mirror*/]; 2 /*perspective*/]);

impl FinnyTable {
    pub fn new(net: &Network) -> Self {
        use std::array::from_fn;
        Self (
            from_fn( |_|
                from_fn( |_|
                    from_fn( |_|
                        FinnyEntry::new(net)
                    )
                )
            )
        )
    }

    pub fn entry_mut(&mut self, perspective: Color, mirror: bool, bucket: usize) -> &mut FinnyEntry {
        &mut self.0[perspective][mirror as usize][bucket]
    }

    // `mirror` and `bucket` are passed in rather than looked up here: both
    // callers have already computed them, and routing every refresh through the
    // one `king_context` call keeps this from becoming a second place that can
    // disagree about which cell a position belongs to.
    pub fn refresh(&mut self, net: &Network, board: &Board, perspective: Color, mirror: bool, bucket: usize) -> Accumulator {
        let entry = self.entry_mut(perspective, mirror, bucket);

        // Counted only under `--features nnue-stats`; both are compiled away
        // otherwise, along with the `record_refresh` call at the bottom.
        #[cfg(feature = "nnue-stats")]
        let (mut diff, mut stale) = (0u64, 0u64);

        for i in 0..Piece::NUM {
            let piece = Piece::from_value(i as u8);
            let curr = board.piece_bb(piece);
            let old = entry.bb[piece];

            let added = curr & !old;
            let removed = old & !curr;

            #[cfg(feature = "nnue-stats")]
            {
                diff += (added.pop_count() + removed.pop_count()) as u64;
                stale += old.pop_count() as u64;
            }

            for sq in added {
                entry.acc += net.feature_weights()[feature_index(perspective, piece, sq, mirror, bucket)];
            }
            for sq in removed {
                entry.acc -= net.feature_weights()[feature_index(perspective, piece, sq, mirror, bucket)];
            }

            entry.bb[piece] = curr
        }

        // A cell is cold when its snapshot held nothing at all, which is the
        // only state `FinnyEntry::new` produces and one no real position can
        // reach -- both kings are always on the board.
        #[cfg(feature = "nnue-stats")]
        crate::nnue::stats::record_refresh(diff, board.occ().pop_count() as u64, stale == 0);

        entry.acc
    }
}

