use crate::board::lookup::{bishop_attack, king_attack, knight_attack, pawn_attack, rook_attack};
use crate::types::uninit_array::UninitArray;
use crate::types::{
    Bitboard, Castling, CastlingKind, Color, File, Move, MoveKind, Piece, PieceType, Rank, Square,
    ZobristHelper,
};

const MAX_STATE_ARRAY_LENGTH: usize = 1024;

#[derive(Copy, Clone)]
pub struct StateInfo {
    pub hash: u64,
    pub half_move: u16,
    pub castling_right: Castling,
    pub en_passant: Square,
    pub captured_piece: Piece,
}

#[derive(Clone)]
pub struct Board {
    pub(super) piece_type_bb: [Bitboard; PieceType::NUM],
    pub(super) color_bb: [Bitboard; Color::NUM],
    pub(super) mailbox: [Piece; Square::NUM],

    pub(super) side_to_move: Color,
    pub(super) castling_right: Castling,
    pub(super) en_passant: Square,
    pub(super) half_move: u16,
    pub(super) zobrist: u64,
    pub(super) game_ply: usize,
    pub(super) piece_count: [u8; Piece::NUM],

    pub(super) state_history: UninitArray<StateInfo, MAX_STATE_ARRAY_LENGTH>,
}

impl Board {
    // phase weight
    pub const GAME_PHASE_INC: [i32; 6] = [0, 1, 1, 2, 4, 0];
    pub const GAME_PHASE_MAX: i32 = 24;

    pub fn from_fen(fen: &str) -> Result<Self, &'static str> {
        let mut board = Board {
            piece_type_bb: [Bitboard::EMPTY; PieceType::NUM],
            color_bb: [Bitboard::EMPTY; Color::NUM],
            mailbox: [Piece::None; Square::NUM],
            side_to_move: Color::White,
            castling_right: Castling::default(),
            en_passant: Square::None,
            half_move: 0,
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
                '/' => {
                    rank -= 1;
                    file = 0;
                }
                '1'..='8' => {
                    file += (ch as u8 - b'0') as i8;
                }
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
            board.zobrist ^= ZobristHelper::ep(board.en_passant);
        }

        // Half move clock
        let half = parts.next().ok_or("Missing half move clock")?;
        board.half_move = half.parse::<u16>().map_err(|_| "Invalid half move clock")?;

        // Full move number (only stored as game_ply; see full_move())
        let full = parts.next().ok_or("Missing full move number")?;
        let full_move = full
            .parse::<usize>()
            .map_err(|_| "Invalid full move number")?;

        board.game_ply = full_move.saturating_sub(1) * 2
            + if board.side_to_move == Color::Black {
                1
            } else {
                0
            };

        Ok(board)
    }

    pub fn start_pos() -> Self {
        Self::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1").unwrap()
    }

    pub fn stm(&self) -> Color {
        self.side_to_move
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

    pub fn full_move(&self) -> usize {
        self.game_ply / 2 + 1
    }

    pub fn hash(&self) -> u64 {
        self.zobrist
    }

    pub fn place_piece(&mut self, piece: Piece, square: Square) {
        self.place_piece_unhashed(piece, square);
        self.zobrist ^= ZobristHelper::square(square, piece);
    }

    pub fn clear_square(&mut self, square: Square) {
        match self.piece_at(square) {
            Piece::None => {}
            piece => self.remove_piece(piece, square),
        }
    }

    fn place_piece_unhashed(&mut self, piece: Piece, square: Square) {
        self.piece_type_bb[piece.piece_type()].set(square);
        self.color_bb[piece.color()].set(square);
        self.mailbox[square] = piece;
        self.piece_count[piece] += 1;
    }

    fn remove_piece_unhashed(&mut self, piece: Piece, square: Square) {
        self.piece_type_bb[piece.piece_type()].clear(square);
        self.color_bb[piece.color()].clear(square);
        self.mailbox[square] = Piece::None;
        self.piece_count[piece] -= 1;
    }

    fn remove_piece(&mut self, piece: Piece, square: Square) {
        self.remove_piece_unhashed(piece, square);
        self.zobrist ^= ZobristHelper::square(square, piece);
    }

    fn move_piece_unhashed(&mut self, piece: Piece, from: Square, to: Square) {
        let mask = Bitboard::from_square(from) | Bitboard::from_square(to);
        self.piece_type_bb[piece.piece_type()] ^= mask;
        self.color_bb[piece.color()] ^= mask;
        self.mailbox[from] = Piece::None;
        self.mailbox[to] = piece;
    }

    fn move_piece(&mut self, piece: Piece, from: Square, to: Square) {
        self.move_piece_unhashed(piece, from, to);
        self.zobrist ^= ZobristHelper::square(from, piece) ^ ZobristHelper::square(to, piece);
    }

    // handle state pushing and popping. ONLY HANDLE STATE PROPERTIES
    fn push_state(&mut self, captured_piece: Piece) {
        self.state_history.push(StateInfo {
            hash: self.zobrist,
            half_move: self.half_move,
            castling_right: self.castling_right,
            en_passant: self.en_passant,
            captured_piece,
        })
    }

    // pop the previous state and return captured piece
    fn pop_state(&mut self) -> Piece {
        let prev_state = self.state_history.pop();

        self.zobrist = prev_state.hash;
        self.half_move = prev_state.half_move;
        self.castling_right = prev_state.castling_right;
        self.en_passant = prev_state.en_passant;

        prev_state.captured_piece
    }

    fn update_castling(&mut self, from: Square, to: Square) {
        let new_rights = Castling::new(
            self.castling_right.raw() as u8
                & Castling::SQUARE_MASK[from]
                & Castling::SQUARE_MASK[to],
        );

        self.zobrist ^=
            ZobristHelper::castling(self.castling_right) ^ ZobristHelper::castling(new_rights);
        self.castling_right = new_rights;
    }

    fn castle_rook_squares(kind: MoveKind, king_to: Square) -> (Square, Square) {
        if matches!(kind, MoveKind::KingCastle) {
            (king_to.offset(1), king_to.offset(-1))
        } else {
            (king_to.offset(-2), king_to.offset(1))
        }
    }

    pub fn make_move(&mut self, mv: Move) {
        let kind = mv.kind();
        let from = mv.from();
        let to = mv.to();
        let piece = self.piece_at(from);
        let captured = self.piece_at(mv.capture_square());

        let us = self.side_to_move;

        self.push_state(captured);

        self.zobrist ^= ZobristHelper::ep(self.en_passant);
        self.en_passant = Square::None;

        self.update_castling(from, to);

        if mv.is_capture() {
            self.remove_piece(captured, mv.capture_square());
        }

        if mv.is_promotion() {
            self.remove_piece(piece, from);
            self.place_piece(Piece::new(us, mv.promo_piece()), to);
        } else {
            self.move_piece(piece, from, to);
        }

        if mv.is_double_push() {
            self.en_passant = to ^ 8; // square behind the pawn
            self.zobrist ^= ZobristHelper::ep(self.en_passant);
        } else if mv.is_castling() {
            let (rook_from, rook_to) = Self::castle_rook_squares(kind, to);
            self.move_piece(Piece::new(us, PieceType::Rook), rook_from, rook_to);
        }

        if mv.is_capture() || piece.piece_type() == PieceType::Pawn {
            self.half_move = 0
        } else {
            self.half_move += 1
        }

        self.game_ply += 1;
        self.side_to_move = !us;
        self.zobrist ^= ZobristHelper::color();
    }

    pub fn unmake_move(&mut self, mv: Move) {
        let captured = self.pop_state();

        let us = !self.side_to_move;
        let kind = mv.kind();
        let from = mv.from();
        let to = mv.to();

        self.game_ply -= 1;
        self.side_to_move = us;

        // Return the mover to its home square.
        if mv.is_promotion() {
            // The promoted piece vanishes and a pawn reappears on `from`.
            self.remove_piece_unhashed(Piece::new(us, mv.promo_piece()), to);
            self.place_piece_unhashed(Piece::new(us, PieceType::Pawn), from);
        } else {
            self.move_piece_unhashed(self.piece_at(to), to, from);
        }

        // Put back whatever was captured (capture_square handles en passant).
        if mv.is_capture() {
            self.place_piece_unhashed(captured, mv.capture_square());
        }

        // Send the rook home for castling.
        if mv.is_castling() {
            let (rook_from, rook_to) = Self::castle_rook_squares(kind, to);
            self.move_piece_unhashed(Piece::new(us, PieceType::Rook), rook_to, rook_from);
        }
    }

    pub fn occ(&self) -> Bitboard {
        self.color_bb(Color::White) | self.color_bb(Color::Black)
    }

    pub fn phase(&self) -> i32 {
        let mut phase = 0;
        for (piece_type, bitboard) in self.piece_type_bb.iter().enumerate() {
            phase += bitboard.pop_count() as i32 * Self::GAME_PHASE_INC[piece_type];
        }
        phase
    }

    pub fn is_check(&self) -> bool {
        !self.checkers(self.side_to_move).is_empty()
    }

    // SEE helper
    pub fn attackers_to(&self, square: Square, occ: Bitboard) -> Bitboard {
        let mut result = Bitboard::EMPTY;
        result |= bishop_attack(occ, square) & (self.piece_type_bb(PieceType::Bishop) | self.piece_type_bb(PieceType::Queen));
        result |= rook_attack(occ, square) & (self.piece_type_bb(PieceType::Rook) | self.piece_type_bb(PieceType::Queen));
        result |= king_attack(square) & self.piece_type_bb(PieceType::King);
        result |= knight_attack(square) & self.piece_type_bb(PieceType::Knight);
        result |= pawn_attack(Color::White, square) & self.piece_bb(Piece::BlackPawn);
        result |= pawn_attack(Color::Black, square) & self.piece_bb(Piece::WhitePawn);

        result & occ
    }

    pub fn make_null_move(&mut self) {
        self.push_state(Piece::None);

        self.zobrist ^= ZobristHelper::ep(self.en_passant);
        self.en_passant = Square::None;

        self.half_move += 1;
        self.game_ply += 1;
        self.side_to_move = !self.side_to_move;
        self.zobrist ^= ZobristHelper::color();
    }

    pub fn unmake_null_move(&mut self) {
        self.pop_state();

        self.game_ply -= 1;
        self.side_to_move = !self.side_to_move;
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    // ----- make_move / unmake_move round-trip tests -----

    // A compact copy of every Board field a move can touch, compared field-by-field
    // so a failure names exactly what diverged.
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
            full_move: b.full_move(),
            game_ply: b.game_ply,
            zobrist: b.zobrist,
            hist_len: b.state_history.len(),
        }
    }

    // Field-by-field so a failure names exactly what diverged.
    fn assert_snapshot_eq(a: &Snapshot, b: &Snapshot, ctx: &str) {
        for i in 0..PieceType::NUM {
            assert_eq!(
                a.piece_type_bb[i], b.piece_type_bb[i],
                "piece_type_bb[{i}] after {ctx}"
            );
        }
        for i in 0..Color::NUM {
            assert_eq!(a.color_bb[i], b.color_bb[i], "color_bb[{i}] after {ctx}");
        }
        for sq in 0..Square::NUM {
            assert_eq!(a.mailbox[sq], b.mailbox[sq], "mailbox[{sq}] after {ctx}");
        }
        for i in 0..Piece::NUM {
            assert_eq!(
                a.piece_count[i], b.piece_count[i],
                "piece_count[{i}] after {ctx}"
            );
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

    // Verify the mailbox and the bitboards tell the same story. The XOR relocation in
    // move_piece is easy to get subtly wrong (e.g. `&` instead of `|` when building the
    // toggle mask) in a way that still round-trips cleanly, so we check the intermediate
    // position directly rather than trusting make/unmake to cancel out.
    fn assert_board_consistent(b: &Board, ctx: &str) {
        let occ = b.color_bb.iter().fold(0u64, |acc, bb| acc | bb.0);
        let pt_occ = b.piece_type_bb.iter().fold(0u64, |acc, bb| acc | bb.0);
        assert_eq!(occ, pt_occ, "color vs piece-type occupancy after {ctx}");

        for sq in 0..Square::NUM {
            let bit = 1u64 << sq;
            let piece = b.mailbox[sq];
            if piece == Piece::None {
                assert_eq!(
                    occ & bit,
                    0,
                    "square {sq} empty in mailbox but set in bitboards after {ctx}"
                );
            } else {
                assert_ne!(
                    b.piece_type_bb[piece.piece_type()].0 & bit,
                    0,
                    "square {sq} missing from its piece_type_bb after {ctx}"
                );
                assert_ne!(
                    b.color_bb[piece.color()].0 & bit,
                    0,
                    "square {sq} missing from its color_bb after {ctx}"
                );
            }
        }
    }

    // make_move then unmake_move must return to the exact starting position.
    fn roundtrip(name: &str, fen: &str, mv: Move) {
        let mut board = Board::from_fen(fen).expect(name);
        let before = snapshot(&board);

        board.make_move(mv);
        // Sanity: every legal move flips the side to move, so the hash must change.
        // Guards against a move that is silently a no-op (which would pass trivially).
        assert_ne!(
            board.zobrist, before.zobrist,
            "make_move changed nothing for {name}"
        );
        // The post-make position must itself be internally consistent, not just reversible.
        assert_board_consistent(&board, name);

        board.unmake_move(mv);
        assert_snapshot_eq(&before, &snapshot(&board), name);
    }

    #[test]
    fn make_unmake_roundtrip_all_kinds() {
        use MoveKind::*;
        let cases: &[(&str, &str, Move)] = &[
            (
                "normal",
                "4k3/8/8/8/8/8/8/4K1N1 w - - 0 1",
                Move::new(Square::G1, Square::F3, Normal),
            ),
            (
                "double_push",
                "4k3/8/8/8/8/8/4P3/4K3 w - - 0 1",
                Move::new(Square::E2, Square::E4, DoublePush),
            ),
            (
                "king_castle_w",
                "4k3/8/8/8/8/8/8/4K2R w K - 0 1",
                Move::new(Square::E1, Square::G1, KingCastle),
            ),
            (
                "queen_castle_w",
                "4k3/8/8/8/8/8/8/R3K3 w Q - 0 1",
                Move::new(Square::E1, Square::C1, QueenCastle),
            ),
            (
                "king_castle_b",
                "4k2r/8/8/8/8/8/8/4K3 b k - 0 1",
                Move::new(Square::E8, Square::G8, KingCastle),
            ),
            (
                "queen_castle_b",
                "r3k3/8/8/8/8/8/8/4K3 b q - 0 1",
                Move::new(Square::E8, Square::C8, QueenCastle),
            ),
            (
                "capture",
                "4k3/8/4n3/8/3N4/8/8/4K3 w - - 0 1",
                Move::new(Square::D4, Square::E6, Capture),
            ),
            (
                "en_passant_w",
                "4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1",
                Move::new(Square::E5, Square::D6, EnPassant),
            ),
            (
                "en_passant_b",
                "4k3/8/8/8/3Pp3/8/8/4K3 b - d3 0 1",
                Move::new(Square::E4, Square::D3, EnPassant),
            ),
            (
                "promo_knight",
                "4k3/P7/8/8/8/8/8/4K3 w - - 0 1",
                Move::new(Square::A7, Square::A8, PromoKnight),
            ),
            (
                "promo_bishop",
                "4k3/P7/8/8/8/8/8/4K3 w - - 0 1",
                Move::new(Square::A7, Square::A8, PromoBishop),
            ),
            (
                "promo_rook",
                "4k3/P7/8/8/8/8/8/4K3 w - - 0 1",
                Move::new(Square::A7, Square::A8, PromoRook),
            ),
            (
                "promo_queen",
                "4k3/P7/8/8/8/8/8/4K3 w - - 0 1",
                Move::new(Square::A7, Square::A8, PromoQueen),
            ),
            (
                "cap_promo_knight",
                "r3k3/1P6/8/8/8/8/8/4K3 w - - 0 1",
                Move::new(Square::B7, Square::A8, CapPromoKnight),
            ),
            (
                "cap_promo_bishop",
                "r3k3/1P6/8/8/8/8/8/4K3 w - - 0 1",
                Move::new(Square::B7, Square::A8, CapPromoBishop),
            ),
            (
                "cap_promo_rook",
                "r3k3/1P6/8/8/8/8/8/4K3 w - - 0 1",
                Move::new(Square::B7, Square::A8, CapPromoRook),
            ),
            (
                "cap_promo_queen",
                "r3k3/1P6/8/8/8/8/8/4K3 w - - 0 1",
                Move::new(Square::B7, Square::A8, CapPromoQueen),
            ),
            // A couple of black-to-move cases to exercise us == Black (game_ply parity, back rank).
            (
                "promo_queen_b",
                "4k3/8/8/8/8/8/p7/4K3 b - - 0 1",
                Move::new(Square::A2, Square::A1, PromoQueen),
            ),
            (
                "cap_promo_q_b",
                "4k3/8/8/8/8/8/1p6/R3K3 b - - 0 1",
                Move::new(Square::B2, Square::A1, CapPromoQueen),
            ),
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

    // make_move's incremental zobrist must match hashing the resulting position
    // from scratch, which is what from_fen does. This catches a missed or extra
    // XOR (ep set/clear, castling rights, promotions, ...) that the round-trip
    // tests cannot see, because unmake restores the hash wholesale from history.
    #[test]
    fn make_move_hash_matches_from_fen() {
        use MoveKind::*;
        let cases: &[(&str, &str, Move, &str)] = &[
            (
                "double_push_sets_ep",
                "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
                Move::new(Square::E2, Square::E4, DoublePush),
                "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1",
            ),
            (
                "king_castle_drops_rights",
                "4k3/8/8/8/8/8/8/4K2R w K - 0 1",
                Move::new(Square::E1, Square::G1, KingCastle),
                "4k3/8/8/8/8/8/8/5RK1 b - - 1 1",
            ),
            (
                "queen_castle_black",
                "r3k3/8/8/8/8/8/8/4K3 b q - 0 1",
                Move::new(Square::E8, Square::C8, QueenCastle),
                "2kr4/8/8/8/8/8/8/4K3 w - - 1 2",
            ),
            (
                "capture",
                "4k3/8/4n3/8/3N4/8/8/4K3 w - - 0 1",
                Move::new(Square::D4, Square::E6, Capture),
                "4k3/8/4N3/8/8/8/8/4K3 b - - 0 1",
            ),
            (
                "en_passant_clears_ep",
                "4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1",
                Move::new(Square::E5, Square::D6, EnPassant),
                "4k3/8/3P4/8/8/8/8/4K3 b - - 0 1",
            ),
            (
                "capture_promotion",
                "r3k3/1P6/8/8/8/8/8/4K3 w - - 0 1",
                Move::new(Square::B7, Square::A8, CapPromoQueen),
                "Q3k3/8/8/8/8/8/8/4K3 b - - 0 1",
            ),
        ];

        for &(name, before, mv, after) in cases {
            let mut board = Board::from_fen(before).expect(name);
            board.make_move(mv);
            let fresh = Board::from_fen(after).expect(name);
            assert_eq!(
                board.zobrist, fresh.zobrist,
                "incremental hash != from-scratch hash for {name}"
            );
        }
    }

    // ----- speed benchmark -----

    // Times a make_move + unmake_move pair, cycling through every MoveKind. Ignored by
    // default so it never slows the normal suite; run it explicitly with:
    //     cargo test bench_make_unmake -- --ignored --nocapture
    #[test]
    #[ignore]
    fn bench_make_unmake() {
        use MoveKind::*;
        use std::hint::black_box;
        use std::time::Instant;

        const ITERATIONS: usize = 100_000_000;

        // One (position, move) per MoveKind. Each make/unmake pair round-trips its own
        // board, so cycling through them keeps every board valid for the whole run and
        // makes the reported time an average across all move types.
        let cases: &[(&str, Move)] = &[
            (
                "4k3/8/8/8/8/8/8/4K1N1 w - - 0 1",
                Move::new(Square::G1, Square::F3, Normal),
            ),
            (
                "4k3/8/8/8/8/8/4P3/4K3 w - - 0 1",
                Move::new(Square::E2, Square::E4, DoublePush),
            ),
            (
                "4k3/8/8/8/8/8/8/4K2R w K - 0 1",
                Move::new(Square::E1, Square::G1, KingCastle),
            ),
            (
                "4k3/8/8/8/8/8/8/R3K3 w Q - 0 1",
                Move::new(Square::E1, Square::C1, QueenCastle),
            ),
            (
                "4k3/8/4n3/8/3N4/8/8/4K3 w - - 0 1",
                Move::new(Square::D4, Square::E6, Capture),
            ),
            (
                "4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1",
                Move::new(Square::E5, Square::D6, EnPassant),
            ),
            (
                "4k3/P7/8/8/8/8/8/4K3 w - - 0 1",
                Move::new(Square::A7, Square::A8, PromoKnight),
            ),
            (
                "4k3/P7/8/8/8/8/8/4K3 w - - 0 1",
                Move::new(Square::A7, Square::A8, PromoBishop),
            ),
            (
                "4k3/P7/8/8/8/8/8/4K3 w - - 0 1",
                Move::new(Square::A7, Square::A8, PromoRook),
            ),
            (
                "4k3/P7/8/8/8/8/8/4K3 w - - 0 1",
                Move::new(Square::A7, Square::A8, PromoQueen),
            ),
            (
                "r3k3/1P6/8/8/8/8/8/4K3 w - - 0 1",
                Move::new(Square::B7, Square::A8, CapPromoKnight),
            ),
            (
                "r3k3/1P6/8/8/8/8/8/4K3 w - - 0 1",
                Move::new(Square::B7, Square::A8, CapPromoBishop),
            ),
            (
                "r3k3/1P6/8/8/8/8/8/4K3 w - - 0 1",
                Move::new(Square::B7, Square::A8, CapPromoRook),
            ),
            (
                "r3k3/1P6/8/8/8/8/8/4K3 w - - 0 1",
                Move::new(Square::B7, Square::A8, CapPromoQueen),
            ),
        ];

        // Boards live on the heap so the bench thread's stack stays small.
        let mut states: Vec<(Board, Move)> = cases
            .iter()
            .map(|&(fen, mv)| (Board::from_fen(fen).expect(fen), mv))
            .collect();
        let kinds = states.len();

        let start = Instant::now();
        // Wrapping counter instead of `i % kinds`: kinds is a runtime value, so
        // the modulo would compile to a hardware div inside the timed loop.
        let mut k = 0;
        for _ in 0..ITERATIONS {
            let (board, mv) = &mut states[k];
            k += 1;
            if k == kinds {
                k = 0;
            }
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
