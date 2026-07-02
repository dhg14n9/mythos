use crate::types::{Bitboard, Castling, CastlingKind, Color, File, Move, MoveKind, Piece, PieceType, Rank, Square, ZobristHelper};
use crate::types::uninit_array::UninitArray;

const MAX_STATE_ARRAY_LENGTH: usize = 16384;

#[derive(Copy, Clone)]
pub struct StateInfo {
    pub castling_right: Castling,
    pub en_passant: Square,
    pub half_move: u16,
    pub full_move: usize,
    pub game_ply: usize,
    pub hash: u64,
    pub captured_piece: Piece
}

#[derive(Clone)]
pub struct Board {
    piece_type_bb: [Bitboard; PieceType::NUM],
    color_bb: [Bitboard; Color::NUM],
    mailbox: [Piece; Square::NUM],

    side_to_move: Color,
    castling_right: Castling,
    en_passant: Square,
    half_move: u16,
    full_move: usize,
    zobrist: u64,
    game_ply: usize,
    piece_count: [u8; Piece::NUM],

    state_history: UninitArray<StateInfo, MAX_STATE_ARRAY_LENGTH>
}

impl Board {
    pub fn from_fen(fen: &str) -> Result<Self, &'static str> {
        let mut board = Board {
            piece_type_bb: [Bitboard::EMPTY; PieceType::NUM],
            color_bb: [Bitboard::EMPTY; Color::NUM],
            mailbox: [Piece::None; Square::NUM],
            side_to_move: Color::White,
            castling_right: Castling::default(),
            en_passant: Square::None,
            half_move: 0,
            full_move: 1,
            zobrist: 0,
            game_ply: 0,
            piece_count: [0; Piece::NUM],
            state_history: UninitArray::new(),
        };

        let mut parts = fen.split_whitespace();

        // Piece placement
        let placement = parts.next().ok_or("Missing piece placement")?;
        let mut rank: i8 = 7;
        let mut file: i8 = 0;
        for ch in placement.chars() {
            match ch {
                '/' => { rank -= 1; file = 0; }
                '1'..='8' => { file += (ch as u8 - b'0') as i8; }
                _ => {
                    let piece = Piece::parse(ch)?;
                    let sq = Square::from_rank_file(Rank::new(rank as u8), File::new(file as u8));
                    board.place_piece(piece, sq);
                    file += 1;
                }
            }
        }

        // Side to move
        let stm = parts.next().ok_or("Missing side to move")?;
        board.side_to_move = Color::parse(stm.chars().next().ok_or("Empty side to move")?)?;
        if board.side_to_move == Color::Black {
            board.zobrist ^= ZobristHelper::color();
        }

        // Castling rights
        let castling = parts.next().ok_or("Missing castling rights")?;
        for ch in castling.chars() {
            match ch {
                'K' => board.castling_right.insert(CastlingKind::WhiteKing),
                'Q' => board.castling_right.insert(CastlingKind::WhiteQueen),
                'k' => board.castling_right.insert(CastlingKind::BlackKing),
                'q' => board.castling_right.insert(CastlingKind::BlackQueen),
                '-' => break,
                _ => return Err("Invalid castling character"),
            }
        }
        board.zobrist ^= ZobristHelper::castling(board.castling_right);

        // En passant
        let ep = parts.next().ok_or("Missing en passant")?;
        if ep != "-" {
            board.en_passant = Square::parse(ep)?;
            board.zobrist ^= ZobristHelper::ep(board.en_passant.file());
        }

        // Half move clock
        let half = parts.next().ok_or("Missing half move clock")?;
        board.half_move = half.parse::<u16>().map_err(|_| "Invalid half move clock")?;

        // Full move number
        let full = parts.next().ok_or("Missing full move number")?;
        board.full_move = full.parse::<usize>().map_err(|_| "Invalid full move number")?;

        board.game_ply = (board.full_move - 1) * 2
            + if board.side_to_move == Color::Black { 1 } else { 0 };

        Ok(board)
    }

    pub fn start_pos() -> Self {
        Self::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1").unwrap()
    }

    pub fn piece_at(&self, square: Square) -> Piece {
        self.mailbox[square]
    }

    pub fn piece_count(&self, piece: Piece) -> u8 {
        self.piece_count[piece]
    }

    pub fn color_bb(&self, color: Color) -> Bitboard {
        self.color_bb[color]
    }

    pub fn piece_type_bb(&self, piece_type: PieceType) -> Bitboard {
        self.piece_type_bb[piece_type]
    }

    pub fn piece_bb(&self, piece: Piece) -> Bitboard {
        self.piece_type_bb[piece.piece_type()] & self.color_bb[piece.color()]
    }

    pub fn place_piece(&mut self, piece: Piece, square: Square) {
        self.piece_type_bb[piece.piece_type()].set(square);
        self.color_bb[piece.color()].set(square);
        self.mailbox[square] = piece;
        self.piece_count[piece] += 1;
        self.zobrist ^= ZobristHelper::square(square, piece);
    }

    pub fn clear_square(&mut self, square: Square) {
        match self.piece_at(square) {
            Piece::None => {  },
            piece => {
                self.piece_type_bb[piece.piece_type()].clear(square);
                self.color_bb[piece.color()].clear(square);
                self.mailbox[square] = Piece::None;
                self.piece_count[piece] -= 1;

                self.zobrist ^= ZobristHelper::square(square, piece);
            }
        }
    }

    // handle state pushing and popping. ONLY HANDLE STATE PROPERTIES
    fn push_state(&mut self, captured_piece: Piece) {
        self.state_history.push(StateInfo {
            castling_right: self.castling_right,
            en_passant: self.en_passant,
            half_move: self.half_move,
            full_move: self.full_move,
            game_ply: self.game_ply,
            hash: self.zobrist,
            captured_piece
        })
    }
    fn pop_state(&mut self) {
        let prev_state = self.state_history.pop();

        self.castling_right = prev_state.castling_right;
        self.en_passant = prev_state.en_passant;
        self.half_move = prev_state.half_move;
        self.full_move = prev_state.full_move;
        self.game_ply = prev_state.game_ply;
        self.zobrist = prev_state.hash;
    }


    fn update_castling(&mut self, from: Square, to: Square) {
        let new_rights = Castling::new(
            self.castling_right.raw() as u8
                & Castling::SQUARE_MASK[from]
                & Castling::SQUARE_MASK[to],
        );

        if new_rights.raw() != self.castling_right.raw() {
            self.zobrist ^= ZobristHelper::castling(self.castling_right);
            self.zobrist ^= ZobristHelper::castling(new_rights);
            self.castling_right = new_rights;
        }
    }

    pub fn make_move(&mut self, mv: Move) {
        let kind = mv.kind();
        let from = mv.from();
        let to = mv.to();
        let piece = self.piece_at(from);

        let us = self.side_to_move;

        self.push_state(self.piece_at(mv.capture_square()));

        self.clear_square(from);

        if !self.en_passant.is_none() {
            self.zobrist ^= ZobristHelper::ep(self.en_passant.file());
            self.en_passant = Square::None;
        }

        self.update_castling(from, to);

        match kind {
            MoveKind::Normal => { self.place_piece(piece, to) }
            MoveKind::DoublePush => {
                self.place_piece(piece, to);
                self.en_passant = Square::ENPASSANT[to];
                self.zobrist ^= ZobristHelper::ep(to.file());
            }
            MoveKind::KingCastle => {
                self.place_piece(
                    Piece::new(us, PieceType::King), 
                    Square::from_rank_file(Rank::FIRST_RANK[us], File::CASTLE_KING_FILE[1])
                ); 
                self.clear_square(Square::from_rank_file(Rank::FIRST_RANK[us], File::START_ROOK_FILE[1])); 
                self.place_piece(
                    Piece::new(us, PieceType::Rook), 
                    Square::from_rank_file(Rank::FIRST_RANK[us], File::CASTLE_ROOK_FILE[1])
                );
            }
            MoveKind::QueenCastle => {
                self.place_piece(
                    Piece::new(us, PieceType::King),
                    Square::from_rank_file(Rank::FIRST_RANK[us], File::CASTLE_KING_FILE[0])
                );
                self.clear_square(Square::from_rank_file(Rank::FIRST_RANK[us], File::START_ROOK_FILE[0]));
                self.place_piece(
                    Piece::new(us, PieceType::Rook),
                    Square::from_rank_file(Rank::FIRST_RANK[us], File::CASTLE_ROOK_FILE[0])
                );
            }
            MoveKind::Capture => {
                self.clear_square(to);
                self.place_piece(piece, to);
            }
            MoveKind::EnPassant => {
                self.place_piece(piece, to);
                self.clear_square(Square::ENPASSANT[to]);
            }
            MoveKind::PromoKnight => { self.place_piece(Piece::new(us, PieceType::Knight), to) }
            MoveKind::PromoBishop => { self.place_piece(Piece::new(us, PieceType::Bishop), to) }
            MoveKind::PromoRook => { self.place_piece(Piece::new(us, PieceType::Rook), to) }
            MoveKind::PromoQueen => { self.place_piece(Piece::new(us, PieceType::Queen), to) }
            MoveKind::CapPromoKnight => {
                self.clear_square(to);
                self.place_piece(Piece::new(us, PieceType::Knight), to)
            }
            MoveKind::CapPromoBishop => {
                self.clear_square(to);
                self.place_piece(Piece::new(us, PieceType::Bishop), to)
            }
            MoveKind::CapPromoRook => {
                self.clear_square(to);
                self.place_piece(Piece::new(us, PieceType::Rook), to)
            }
            MoveKind::CapPromoQueen => {
                self.clear_square(to);
                self.place_piece(Piece::new(us, PieceType::Queen), to)
            }
        }
        if mv.is_capture() || piece.piece_type() == PieceType::Pawn {
            self.half_move = 0
        } else { 
            self.half_move += 1
        }

        self.game_ply += 1;
        if us == Color::Black {
            self.full_move += 1;
        }
        
        self.side_to_move = !us;
        self.zobrist ^= ZobristHelper::color();
    }



}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    // Benchmark: apply 10,000,000 make_move calls and report the elapsed time.
    // For a meaningful number, run in release:
    //     cargo test --release bench_make_move -- --nocapture
    #[test]
    fn bench_make_move() {
        const ITERATIONS: usize = 10_000_000;

        let mut board = Board::start_pos();

        // A reversible pair of knight moves (g1 <-> h3). Applying them alternately
        // leaves the board's piece placement untouched after every two moves, so we
        // can hammer make_move without ever reaching an illegal position.
        let forward = Move::new(Square::G1, Square::H3, MoveKind::Normal);
        let backward = Move::new(Square::H3, Square::G1, MoveKind::Normal);

        let start = Instant::now();
        for i in 0..ITERATIONS {
            board.make_move(if i & 1 == 0 { forward } else { backward });

            // make_move pushes onto the fixed-size state history (and never pops here),
            // while the half-move clock keeps climbing. Reset both so the history never
            // overflows and the u16 clock never wraps across all 10M iterations.
            board.state_history.clear();
            board.half_move = 0;
        }
        let elapsed = start.elapsed();

        let per_move = elapsed.as_nanos() as f64 / ITERATIONS as f64;
        let moves_per_sec = ITERATIONS as f64 / elapsed.as_secs_f64();
        println!(
            "make_move x{ITERATIONS}: {elapsed:?} ({per_move:.2} ns/move, {moves_per_sec:.0} moves/s)"
        );
    }
}
