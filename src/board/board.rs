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

    // pop the previous state and return captured piece
    fn pop_state(&mut self) -> Piece {
        let prev_state = self.state_history.pop();

        self.castling_right = prev_state.castling_right;
        self.en_passant = prev_state.en_passant;
        self.half_move = prev_state.half_move;
        self.full_move = prev_state.full_move;
        self.game_ply = prev_state.game_ply;
        self.zobrist = prev_state.hash;

        prev_state.captured_piece
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

    pub fn unmake_move(&mut self, mv: Move) {
        let captured =self.pop_state();
        let restored_hash = self.zobrist;

        let us = !self.side_to_move;
        let kind = mv.kind();
        let from = mv.from();
        let to = mv.to();
        let piece = if mv.is_promotion() { Piece::new(us, PieceType::Pawn) } else { self.piece_at(to) };


        // return to home square
        self.clear_square(to);
        self.place_piece(piece, from);

        if mv.is_enpassant() {
            self.place_piece(Piece::new(!us, PieceType::Pawn), Square::ENPASSANT[to])
        } else if mv.is_capture() {
            self.place_piece(captured, to)
        }

        if mv.is_castling() {
            let rook_from: Square;
            let rook_to: Square;

            if matches!(kind, MoveKind::KingCastle) {
                rook_from = Square::from_rank_file(Rank::FIRST_RANK[us], File::START_ROOK_FILE[1]);
                rook_to = Square::from_rank_file(Rank::FIRST_RANK[us], File::CASTLE_ROOK_FILE[1]);
            }
            else {
                rook_from = Square::from_rank_file(Rank::FIRST_RANK[us], File::START_ROOK_FILE[0]);
                rook_to = Square::from_rank_file(Rank::FIRST_RANK[us], File::CASTLE_ROOK_FILE[0]);
            }

            self.clear_square(rook_to);
            self.place_piece(Piece::new(us, PieceType::Rook), rook_from);
        }

        self.zobrist = restored_hash;
        self.side_to_move = us;
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    // ----- make_move / unmake_move round-trip tests -----

    // A compact copy of every Board field a move can touch. Board itself is huge
    // (~640 KB because of the inline state_history), so we snapshot only these small
    // fields instead of cloning the whole board onto the test thread's stack.
    struct Snapshot {
        piece_type_bb: [u64; PieceType::NUM],
        color_bb: [u64; Color::NUM],
        mailbox: [u8; Square::NUM],
        piece_count: [u8; Piece::NUM],
        side_to_move: u8,
        castling: usize,
        en_passant: u8,
        half_move: u16,
        full_move: usize,
        game_ply: usize,
        zobrist: u64,
        hist_len: usize,
    }

    fn snapshot(b: &Board) -> Snapshot {
        let mut piece_type_bb = [0u64; PieceType::NUM];
        for i in 0..PieceType::NUM {
            piece_type_bb[i] = b.piece_type_bb[i].0;
        }
        let mut color_bb = [0u64; Color::NUM];
        for i in 0..Color::NUM {
            color_bb[i] = b.color_bb[i].0;
        }
        let mut mailbox = [0u8; Square::NUM];
        for i in 0..Square::NUM {
            mailbox[i] = b.mailbox[i] as u8;
        }
        let mut piece_count = [0u8; Piece::NUM];
        for i in 0..Piece::NUM {
            piece_count[i] = b.piece_count[i];
        }
        Snapshot {
            piece_type_bb,
            color_bb,
            mailbox,
            piece_count,
            side_to_move: b.side_to_move as u8,
            castling: b.castling_right.raw(),
            en_passant: b.en_passant as u8,
            half_move: b.half_move,
            full_move: b.full_move,
            game_ply: b.game_ply,
            zobrist: b.zobrist,
            hist_len: b.state_history.len(),
        }
    }

    // Field-by-field so a failure names exactly what diverged.
    fn assert_snapshot_eq(a: &Snapshot, b: &Snapshot, ctx: &str) {
        for i in 0..PieceType::NUM {
            assert_eq!(a.piece_type_bb[i], b.piece_type_bb[i], "piece_type_bb[{i}] after {ctx}");
        }
        for i in 0..Color::NUM {
            assert_eq!(a.color_bb[i], b.color_bb[i], "color_bb[{i}] after {ctx}");
        }
        for sq in 0..Square::NUM {
            assert_eq!(a.mailbox[sq], b.mailbox[sq], "mailbox[{sq}] after {ctx}");
        }
        for i in 0..Piece::NUM {
            assert_eq!(a.piece_count[i], b.piece_count[i], "piece_count[{i}] after {ctx}");
        }
        assert_eq!(a.side_to_move, b.side_to_move, "side_to_move after {ctx}");
        assert_eq!(a.castling, b.castling, "castling after {ctx}");
        assert_eq!(a.en_passant, b.en_passant, "en_passant after {ctx}");
        assert_eq!(a.half_move, b.half_move, "half_move after {ctx}");
        assert_eq!(a.full_move, b.full_move, "full_move after {ctx}");
        assert_eq!(a.game_ply, b.game_ply, "game_ply after {ctx}");
        assert_eq!(a.zobrist, b.zobrist, "zobrist after {ctx}");
        assert_eq!(a.hist_len, b.hist_len, "state_history len after {ctx}");
    }

    // make_move then unmake_move must return to the exact starting position.
    fn roundtrip(name: &str, fen: &str, mv: Move) {
        let mut board = Board::from_fen(fen).expect(name);
        let before = snapshot(&board);

        board.make_move(mv);
        // Sanity: every legal move flips the side to move, so the hash must change.
        // Guards against a move that is silently a no-op (which would pass trivially).
        assert_ne!(board.zobrist, before.zobrist, "make_move changed nothing for {name}");

        board.unmake_move(mv);
        assert_snapshot_eq(&before, &snapshot(&board), name);
    }

    #[test]
    fn make_unmake_roundtrip_all_kinds() {
        use MoveKind::*;
        let cases: &[(&str, &str, Move)] = &[
            ("normal",          "4k3/8/8/8/8/8/8/4K1N1 w - - 0 1",    Move::new(Square::G1, Square::F3, Normal)),
            ("double_push",     "4k3/8/8/8/8/8/4P3/4K3 w - - 0 1",    Move::new(Square::E2, Square::E4, DoublePush)),
            ("king_castle_w",   "4k3/8/8/8/8/8/8/4K2R w K - 0 1",     Move::new(Square::E1, Square::G1, KingCastle)),
            ("queen_castle_w",  "4k3/8/8/8/8/8/8/R3K3 w Q - 0 1",     Move::new(Square::E1, Square::C1, QueenCastle)),
            ("king_castle_b",   "4k2r/8/8/8/8/8/8/4K3 b k - 0 1",     Move::new(Square::E8, Square::G8, KingCastle)),
            ("queen_castle_b",  "r3k3/8/8/8/8/8/8/4K3 b q - 0 1",     Move::new(Square::E8, Square::C8, QueenCastle)),
            ("capture",         "4k3/8/4n3/8/3N4/8/8/4K3 w - - 0 1",  Move::new(Square::D4, Square::E6, Capture)),
            ("en_passant_w",    "4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1",  Move::new(Square::E5, Square::D6, EnPassant)),
            ("en_passant_b",    "4k3/8/8/8/3Pp3/8/8/4K3 b - d3 0 1",  Move::new(Square::E4, Square::D3, EnPassant)),
            ("promo_knight",    "4k3/P7/8/8/8/8/8/4K3 w - - 0 1",     Move::new(Square::A7, Square::A8, PromoKnight)),
            ("promo_bishop",    "4k3/P7/8/8/8/8/8/4K3 w - - 0 1",     Move::new(Square::A7, Square::A8, PromoBishop)),
            ("promo_rook",      "4k3/P7/8/8/8/8/8/4K3 w - - 0 1",     Move::new(Square::A7, Square::A8, PromoRook)),
            ("promo_queen",     "4k3/P7/8/8/8/8/8/4K3 w - - 0 1",     Move::new(Square::A7, Square::A8, PromoQueen)),
            ("cap_promo_knight","r3k3/1P6/8/8/8/8/8/4K3 w - - 0 1",   Move::new(Square::B7, Square::A8, CapPromoKnight)),
            ("cap_promo_bishop","r3k3/1P6/8/8/8/8/8/4K3 w - - 0 1",   Move::new(Square::B7, Square::A8, CapPromoBishop)),
            ("cap_promo_rook",  "r3k3/1P6/8/8/8/8/8/4K3 w - - 0 1",   Move::new(Square::B7, Square::A8, CapPromoRook)),
            ("cap_promo_queen", "r3k3/1P6/8/8/8/8/8/4K3 w - - 0 1",   Move::new(Square::B7, Square::A8, CapPromoQueen)),
            // A couple of black-to-move cases to exercise us == Black (full_move bump, back rank).
            ("promo_queen_b",   "4k3/8/8/8/8/8/p7/4K3 b - - 0 1",     Move::new(Square::A2, Square::A1, PromoQueen)),
            ("cap_promo_q_b",   "4k3/8/8/8/8/8/1p6/R3K3 b - - 0 1",   Move::new(Square::B2, Square::A1, CapPromoQueen)),
        ];

        for &(name, fen, mv) in cases {
            roundtrip(name, fen, mv);
        }
    }

    #[test]
    fn make_unmake_sequence() {
        // Play a short line from the start position, then unmake it in reverse.
        // Exercises the state-history stack across several stacked make/unmake pairs.
        let mut board = Board::start_pos();
        let start = snapshot(&board);

        let moves = [
            Move::new(Square::E2, Square::E4, MoveKind::DoublePush),
            Move::new(Square::D7, Square::D5, MoveKind::DoublePush),
            Move::new(Square::E4, Square::D5, MoveKind::Capture),
            Move::new(Square::G8, Square::F6, MoveKind::Normal),
            Move::new(Square::G1, Square::F3, MoveKind::Normal),
        ];

        for mv in moves {
            board.make_move(mv);
        }
        for mv in moves.iter().rev() {
            board.unmake_move(*mv);
        }

        assert_snapshot_eq(&start, &snapshot(&board), "sequence");
    }

    // ----- speed benchmark -----

    // Times a make_move + unmake_move pair, cycling through every MoveKind. Ignored by
    // default so it never slows the normal suite; run it explicitly with:
    //     cargo test bench_make_unmake -- --ignored --nocapture
    #[test]
    #[ignore]
    fn bench_make_unmake() {
        use std::hint::black_box;
        use std::time::Instant;
        use MoveKind::*;

        const ITERATIONS: usize = 100_000_000;

        // One (position, move) per MoveKind. Each make/unmake pair round-trips its own
        // board, so cycling through them keeps every board valid for the whole run and
        // makes the reported time an average across all move types.
        let cases: &[(&str, Move)] = &[
            ("4k3/8/8/8/8/8/8/4K1N1 w - - 0 1",   Move::new(Square::G1, Square::F3, Normal)),
            ("4k3/8/8/8/8/8/4P3/4K3 w - - 0 1",   Move::new(Square::E2, Square::E4, DoublePush)),
            ("4k3/8/8/8/8/8/8/4K2R w K - 0 1",    Move::new(Square::E1, Square::G1, KingCastle)),
            ("4k3/8/8/8/8/8/8/R3K3 w Q - 0 1",    Move::new(Square::E1, Square::C1, QueenCastle)),
            ("4k3/8/4n3/8/3N4/8/8/4K3 w - - 0 1", Move::new(Square::D4, Square::E6, Capture)),
            ("4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1", Move::new(Square::E5, Square::D6, EnPassant)),
            ("4k3/P7/8/8/8/8/8/4K3 w - - 0 1",    Move::new(Square::A7, Square::A8, PromoKnight)),
            ("4k3/P7/8/8/8/8/8/4K3 w - - 0 1",    Move::new(Square::A7, Square::A8, PromoBishop)),
            ("4k3/P7/8/8/8/8/8/4K3 w - - 0 1",    Move::new(Square::A7, Square::A8, PromoRook)),
            ("4k3/P7/8/8/8/8/8/4K3 w - - 0 1",    Move::new(Square::A7, Square::A8, PromoQueen)),
            ("r3k3/1P6/8/8/8/8/8/4K3 w - - 0 1",  Move::new(Square::B7, Square::A8, CapPromoKnight)),
            ("r3k3/1P6/8/8/8/8/8/4K3 w - - 0 1",  Move::new(Square::B7, Square::A8, CapPromoBishop)),
            ("r3k3/1P6/8/8/8/8/8/4K3 w - - 0 1",  Move::new(Square::B7, Square::A8, CapPromoRook)),
            ("r3k3/1P6/8/8/8/8/8/4K3 w - - 0 1",  Move::new(Square::B7, Square::A8, CapPromoQueen)),
        ];

        // Boards live on the heap (each Board is ~640 KB; 14 on the stack would overflow it).
        let mut states: Vec<(Board, Move)> = cases
            .iter()
            .map(|&(fen, mv)| (Board::from_fen(fen).expect(fen), mv))
            .collect();
        let kinds = states.len();

        let start = Instant::now();
        for i in 0..ITERATIONS {
            let (board, mv) = &mut states[i % kinds];
            // black_box stops the optimizer from proving the pair is a no-op and
            // deleting the whole loop, which would make the timing meaningless.
            let mv = black_box(*mv);
            board.make_move(mv);
            board.unmake_move(mv);
            black_box(&*board);
        }
        let elapsed = start.elapsed();

        // One iteration == one make + one unmake.
        let per_pair = elapsed.as_nanos() as f64 / ITERATIONS as f64;
        let pairs_per_sec = ITERATIONS as f64 / elapsed.as_secs_f64();
        println!(
            "make+unmake x{ITERATIONS} over {kinds} move kinds: {elapsed:?} \
             ({per_pair:.2} ns/pair, {pairs_per_sec:.0} pairs/s)"
        );
    }
}
