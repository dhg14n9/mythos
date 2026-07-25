use crate::types::{Color, PieceType, Square};

pub const PSQT:          usize = 0;   // 384
pub const TEMPO:         usize = 384;
pub const BISHOP_PAIR:   usize = 385;
pub const PASSED_PAWN:   usize = 386; // 8
pub const ISOLATED_PAWN: usize = 394;
pub const DOUBLED_PAWN:  usize = 395;
pub const KNIGHT_MOB:    usize = 396; // 9
pub const BISHOP_MOB:    usize = 405; // 14
pub const ROOK_MOB:      usize = 419; // 15
pub const QUEEN_MOB:     usize = 434; // 28
pub const NUM_PARAMS:    usize = 462;

pub struct Trace { array: [i16; NUM_PARAMS] }

impl Trace {
    pub fn new() -> Self {
        Self { array: [0; NUM_PARAMS] }
    }

    pub fn coeffs(&self) -> &[i16; NUM_PARAMS] {
        &self.array
    }

    pub fn add(&mut self, index: usize, color: Color, n: i16) {
        self.array[index] += n * match color {
            Color::White => {1}
            Color::Black => {-1}
        }
    }

    pub fn sub(&mut self, index: usize, color: Color, n: i16) {
        self.add(index, color, -n);
    }

    fn psqt(&mut self, square: Square, color: Color, piece_type: PieceType) {
        debug_assert!(piece_type != PieceType::None);

        let sq = square.rev_relative_to(color) as usize;
        let pt = piece_type as usize;
        self.add(PSQT + pt * 64 + sq, color, 1)
    }


}


