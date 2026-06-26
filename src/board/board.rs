
use crate::types::{Bitboard, Castling, CastlingKind, Color, File, Piece, PieceType, Rank, Square, ZobristHelper};

#[derive(Copy, Clone)]
pub struct StateInfo {
    pub castling_right: Castling,
    pub en_passant: Square,
    pub half_move: u16,
    pub hash: u64
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
            ZobristHelper::toggle_color(&mut board.zobrist);
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
        ZobristHelper::toggle_castling(&mut board.zobrist, board.castling_right);

        // En passant
        let ep = parts.next().ok_or("Missing en passant")?;
        if ep != "-" {
            board.en_passant = Square::parse(ep)?;
            ZobristHelper::toggle_ep(&mut board.zobrist, board.en_passant.file());
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
        ZobristHelper::toggle_square(&mut self.zobrist, square, piece);
    }

    pub fn clear_square(&mut self, square: Square) {
        match self.piece_at(square) {
            Piece::None => {  },
            piece => {
                self.piece_type_bb[piece.piece_type()].clear(square);
                self.color_bb[piece.color()].clear(square);
                self.mailbox[square] = Piece::None;
                self.piece_count[piece] -= 1;
            }
        }
    }

}
