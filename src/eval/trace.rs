use crate::board::board::Board;
use crate::eval::eval::{BISHOP_PAIR_BONUS, TEMPO_BONUS};
use crate::eval::king_safety::king_safety;
use crate::eval::mobility::{
    BISHOP_SEED, KNIGHT_MOBILITY, QUEEN_SEED, ROOK_SEED,
    bishop_mobility, knight_mobility, pawn_attack, queen_mobility, rook_mobility,
};
use crate::eval::pawn::{DOUBLE_PAWN_MALUS, ISO_PAWN_MALUS, PASS_PAWN_BONUS};
use crate::eval::piece_square::TABLES;
use crate::eval::{S, spread};
use crate::types::{Color, Piece, PieceType, Square};

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

pub struct Trace {
    array: [i16; NUM_PARAMS],
    frozen: S,
    phase: i32,
}

impl Trace {
    pub fn new() -> Self {
        Self { array: [0; NUM_PARAMS], frozen: S(0, 0), phase: 0 }
    }

    pub fn coeffs(&self) -> &[i16; NUM_PARAMS] {
        &self.array
    }

    // King safety, untapered so the equivalence test can stay exact.
    pub fn frozen(&self) -> S {
        self.frozen
    }

    pub fn phase(&self) -> i32 {
        self.phase
    }

    pub fn add(&mut self, index: usize, color: Color, n: i16) {
        self.array[index] += n * match color {
            Color::White => {1}
            Color::Black => {-1}
        }
    }

    // Mirrors `-=` in the eval: a term that is subtracted has a negated coefficient.
    pub fn sub(&mut self, index: usize, color: Color, n: i16) {
        self.add(index, color, -n);
    }

    pub fn trace(&mut self, board: &Board) {
        self.psqt(board);
        self.tempo(board);
        self.bishop_pair(board);
        self.pawns(board);
        self.mobility(board);

        self.frozen = king_safety(board);
        self.phase = board.phase();
    }

    fn psqt(&mut self, board: &Board) {
        for color in Color::ALL {
            for piece_type in PieceType::ALL {
                for square in board.piece_bb(Piece::new(color, piece_type)) {
                    self.psqt_piece(square, color, piece_type);
                }
            }
        }
    }

    fn psqt_piece(&mut self, square: Square, color: Color, piece_type: PieceType) {
        debug_assert!(piece_type != PieceType::None);

        let sq = square.rev_relative_to(color) as usize;
        let pt = piece_type as usize;
        self.add(PSQT + pt * 64 + sq, color, 1)
    }

    fn tempo(&mut self, board: &Board) {
        self.add(TEMPO, board.stm(), 1);
    }

    fn bishop_pair(&mut self, board: &Board) {
        for color in Color::ALL {
            if board.piece_bb(Piece::new(color, PieceType::Bishop)).pop_count() >= 2 {
                self.add(BISHOP_PAIR, color, 1);
            }
        }
    }

    fn pawns(&mut self, board: &Board) {
        let pawn_bb = [board.piece_bb(Piece::WhitePawn), board.piece_bb(Piece::BlackPawn)];

        for color in Color::ALL {
            let own = pawn_bb[color];
            let opp = pawn_bb[!color];

            let own_north = own.north_fill();
            let own_south = own.south_fill();

            let opp_wide = opp | spread(opp);

            let (own_ahead, opp_ahead) = match color {
                Color::White => (own_south >> 8, opp_wide.south_fill() >> 8),
                Color::Black => (own_north << 8, opp_wide.north_fill() << 8),
            };

            for sq in own & !own_ahead & !opp_ahead {
                self.add(PASSED_PAWN + sq.relative_to(color).rank() as usize, color, 1);
            }

            let iso = (own & !spread(own_north | own_south)).pop_count() as i16;
            self.sub(ISOLATED_PAWN, color, iso);

            let doubled = (own & own_ahead).pop_count() as i16;
            self.sub(DOUBLED_PAWN, color, doubled);
        }
    }

    fn mobility(&mut self, board: &Board) {
        for color in Color::ALL {
            let them_pawn_attack = pawn_attack(board.piece_bb(Piece::new(!color, PieceType::Pawn)), !color);

            // queen
            for sq in board.piece_bb(Piece::new(color, PieceType::Queen)) {
                self.add(QUEEN_MOB + queen_mobility(sq, board, them_pawn_attack).pop_count(), color, 1);
            }

            // rook
            for sq in board.piece_bb(Piece::new(color, PieceType::Rook)) {
                self.add(ROOK_MOB + rook_mobility(sq, board, them_pawn_attack).pop_count(), color, 1);
            }

            // knight
            for sq in board.piece_bb(Piece::new(color, PieceType::Knight)) {
                self.add(KNIGHT_MOB + knight_mobility(sq, board, them_pawn_attack).pop_count(), color, 1);
            }

            // bishop
            for sq in board.piece_bb(Piece::new(color, PieceType::Bishop)) {
                self.add(BISHOP_MOB + bishop_mobility(sq, board, them_pawn_attack).pop_count(), color, 1);
            }
        }
    }
}

// The eval's current weights, laid out in the flat index space above.
pub fn initial_weights() -> [S; NUM_PARAMS] {
    let mut weights = [S(0, 0); NUM_PARAMS];

    for pt in 0..PieceType::NUM {
        for sq in 0..Square::NUM {
            weights[PSQT + pt * 64 + sq] = TABLES[pt][sq];
        }
    }

    weights[TEMPO] = TEMPO_BONUS;
    weights[BISHOP_PAIR] = BISHOP_PAIR_BONUS;

    weights[PASSED_PAWN..PASSED_PAWN + 8].copy_from_slice(&PASS_PAWN_BONUS);
    weights[ISOLATED_PAWN] = ISO_PAWN_MALUS;
    weights[DOUBLED_PAWN] = DOUBLE_PAWN_MALUS;

    weights[KNIGHT_MOB..KNIGHT_MOB + 9].copy_from_slice(&KNIGHT_MOBILITY);
    weights[BISHOP_MOB..BISHOP_MOB + 14].copy_from_slice(&BISHOP_SEED);
    weights[ROOK_MOB..ROOK_MOB + 15].copy_from_slice(&ROOK_SEED);
    weights[QUEEN_MOB..QUEEN_MOB + 28].copy_from_slice(&QUEEN_SEED);

    weights
}
