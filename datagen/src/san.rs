// SAN -> Move.
//
// No SAN parser is written here in the sense of rebuilding the notation: the SAN
// is decomposed into constraints (piece, destination, promotion, and whichever
// half of the origin square was given) and applied as a filter over the legal
// move list, the same trick `find_move` in src/uci.rs uses for UCI strings.
// Exactly one survivor is required — zero or two is an error, never a guess,
// because a wrong move here silently corrupts every position after it.

use mythos::board::board::Board;
use mythos::types::{File, Move, MoveKind, MoveList, PieceType, Rank, Square};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SanError {
    Malformed,
    NoMatch,
    Ambiguous,
}

impl std::fmt::Display for SanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            SanError::Malformed => "malformed SAN",
            SanError::NoMatch => "no legal move matches",
            SanError::Ambiguous => "more than one legal move matches",
        };
        write!(f, "{s}")
    }
}

pub fn find_move(board: &Board, san: &str) -> Result<Move, SanError> {
    // Check, mate and annotation glyphs carry nothing we need.
    let core = san.trim_end_matches(['+', '#', '!', '?']);

    let mut list = MoveList::new();
    board.gen_move(&mut list, false);

    if let Some(queenside) = castling_side(core) {
        let want = if queenside { MoveKind::QueenCastle } else { MoveKind::KingCastle } as u8;
        for i in 0..list.len() {
            let mv = list.get_nth(i);
            if mv.is_castling() && mv.kind() as u8 == want {
                return Ok(mv);
            }
        }
        return Err(SanError::NoMatch);
    }

    // `e8=Q` / `exd8=Q`
    let (core, promo) = match core.rfind('=') {
        Some(i) => {
            let ch = core[i + 1..].chars().next().ok_or(SanError::Malformed)?;
            let pt = PieceType::parse(ch).map_err(|_| SanError::Malformed)?;
            (&core[..i], Some(pt))
        }
        None => (core, None),
    };

    // A leading uppercase letter names the piece; anything else is a pawn move.
    // Note `b` is a file, `B` is a bishop.
    let (piece, rest) = match core.as_bytes().first() {
        Some(b'N') => (PieceType::Knight, &core[1..]),
        Some(b'B') => (PieceType::Bishop, &core[1..]),
        Some(b'R') => (PieceType::Rook, &core[1..]),
        Some(b'Q') => (PieceType::Queen, &core[1..]),
        Some(b'K') => (PieceType::King, &core[1..]),
        _ => (PieceType::Pawn, core),
    };

    // The destination is always the final two characters.
    if rest.len() < 2 {
        return Err(SanError::Malformed);
    }
    let (head, dest) = rest.split_at(rest.len() - 2);
    let dest = Square::parse(dest).map_err(|_| SanError::Malformed)?;

    // What is left is the capture marker plus 0-2 disambiguation characters.
    let mut from_file = None;
    let mut from_rank = None;
    for ch in head.chars() {
        match ch {
            'x' => {}
            'a'..='h' => from_file = Some(File::new(ch as u8 - b'a')),
            '1'..='8' => from_rank = Some(Rank::new(ch as u8 - b'1')),
            _ => return Err(SanError::Malformed),
        }
    }

    let mut found = None;
    for i in 0..list.len() {
        let mv = list.get_nth(i);

        // A castling move is a king move to g1/c1 and would otherwise answer to
        // `Kg1`; only the O-O forms above may select one.
        if mv.is_castling() || mv.to() != dest {
            continue;
        }
        if board.piece_at(mv.from()).piece_type() != piece {
            continue;
        }
        match promo {
            Some(pt) => {
                if !mv.is_promotion() || mv.promo_piece() != pt {
                    continue;
                }
            }
            None => {
                if mv.is_promotion() {
                    continue;
                }
            }
        }
        if from_file.is_some_and(|f| mv.from().file() != f) {
            continue;
        }
        if from_rank.is_some_and(|r| mv.from().rank() != r) {
            continue;
        }

        if found.is_some() {
            return Err(SanError::Ambiguous);
        }
        found = Some(mv);
    }

    found.ok_or(SanError::NoMatch)
}

fn castling_side(core: &str) -> Option<bool> {
    match core {
        "O-O" | "0-0" => Some(false),
        "O-O-O" | "0-0-0" => Some(true),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolve(fen: &str, san: &str) -> Result<String, SanError> {
        let board = Board::from_fen(fen).expect("test fen");
        find_move(&board, san).map(|mv| mv.to_string())
    }

    const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

    #[test]
    fn plain_moves() {
        assert_eq!(resolve(START, "e4").unwrap(), "e2e4");
        assert_eq!(resolve(START, "Nf3").unwrap(), "g1f3");
        assert_eq!(resolve(START, "a3").unwrap(), "a2a3");
    }

    #[test]
    fn lowercase_b_is_a_file_not_a_bishop() {
        // b4 is a pawn push; Bb4 would need the bishop, which is boxed in here.
        assert_eq!(resolve(START, "b4").unwrap(), "b2b4");
        assert_eq!(resolve(START, "Bb5"), Err(SanError::NoMatch));
    }

    #[test]
    fn captures_and_check_suffixes() {
        let fen = "rnbqkbnr/ppp2p1p/8/3pp1p1/2PP4/N6N/PP2PPPP/R1BQKB1R w KQkq - 0 5";
        assert_eq!(resolve(fen, "dxe5").unwrap(), "d4e5");
        assert_eq!(resolve(fen, "cxd5").unwrap(), "c4d5");
        let after = "rnbqkbnr/ppp2p1p/8/3pP1p1/2P5/N6N/PP2PPPP/R1BQKB1R b KQkq - 0 5";
        assert_eq!(resolve(after, "Bb4+").unwrap(), "f8b4");
    }

    #[test]
    fn file_disambiguation() {
        let fen = "4k3/8/8/8/4K3/8/8/R6R w - - 0 1";
        assert_eq!(resolve(fen, "Rad1").unwrap(), "a1d1");
        assert_eq!(resolve(fen, "Rhd1").unwrap(), "h1d1");
        // Without it the rook move is genuinely ambiguous.
        assert_eq!(resolve(fen, "Rd1"), Err(SanError::Ambiguous));
    }

    #[test]
    fn rank_disambiguation() {
        let fen = "R7/8/8/4k3/8/8/8/R3K3 w Q - 0 1";
        assert_eq!(resolve(fen, "R1a5").unwrap(), "a1a5");
        assert_eq!(resolve(fen, "R8a5").unwrap(), "a8a5");
        assert_eq!(resolve(fen, "Ra5"), Err(SanError::Ambiguous));
    }

    #[test]
    fn castling() {
        let fen = "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1";
        assert_eq!(resolve(fen, "O-O").unwrap(), "e1g1");
        assert_eq!(resolve(fen, "O-O-O").unwrap(), "e1c1");
        let black = "r3k2r/8/8/8/8/8/8/R3K2R b KQkq - 0 1";
        assert_eq!(resolve(black, "O-O").unwrap(), "e8g8");
        assert_eq!(resolve(black, "O-O-O+").unwrap(), "e8c8");
    }

    #[test]
    fn promotions() {
        let fen = "1n5k/P7/8/8/8/8/8/K7 w - - 0 1";
        assert_eq!(resolve(fen, "a8=Q").unwrap(), "a7a8q");
        assert_eq!(resolve(fen, "a8=N").unwrap(), "a7a8n");
        assert_eq!(resolve(fen, "axb8=Q+").unwrap(), "a7b8q");
        assert_eq!(resolve(fen, "axb8=R").unwrap(), "a7b8r");
        // A promotion is never the answer to a SAN that names no promotion piece.
        assert_eq!(resolve(fen, "a8"), Err(SanError::NoMatch));
    }

    #[test]
    fn en_passant() {
        let fen = "4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 2";
        assert_eq!(resolve(fen, "exd6").unwrap(), "e5d6");
    }

    #[test]
    fn king_move_is_not_castling() {
        let fen = "4k3/8/8/8/8/8/8/R3K2R w KQ - 0 1";
        assert_eq!(resolve(fen, "Kf1").unwrap(), "e1f1");
        assert_eq!(resolve(fen, "Kd1").unwrap(), "e1d1");
    }

    #[test]
    fn ambiguous_and_illegal_are_errors() {
        let fen = "4k3/8/8/8/8/5N2/8/1N2K3 w - - 0 1";
        assert_eq!(resolve(fen, "Nd2"), Err(SanError::Ambiguous));
        assert_eq!(resolve(fen, "Nd3"), Err(SanError::NoMatch));
        assert_eq!(resolve(fen, "zz"), Err(SanError::Malformed));
    }
}
