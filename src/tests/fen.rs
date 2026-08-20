use crate::board::board::Board;
use crate::types::MoveList;

// to_fen is the inverse of from_fen, so the suite FENs are free test data:
// every one of them must survive a round trip byte for byte. That covers
// castling order, en passant formatting and the move-number inversion.
#[test]
fn suite_fens_round_trip() {
    for (fen, _, _) in crate::bench::cases() {
        let board = Board::from_fen(fen).unwrap_or_else(|e| panic!("bad FEN {fen}: {e}"));
        assert_eq!(board.to_fen(), fen);
    }
}

// game_ply and half_move only move under make_move, so string equality against
// a hand-written FEN cannot reach them. Compare Zobrist keys instead: equal
// hashes prove placement, side to move, castling rights and the en passant
// square all survived the trip through text.
#[test]
fn round_trip_after_moves() {
    let mut board = Board::start_pos();

    for uci in ["e2e4", "c7c5", "g1f3", "b8c6", "f1b5", "g8f6", "e1g1", "f6e4"] {
        let mv = find_move(&board, uci).unwrap_or_else(|| panic!("no legal move {uci}"));
        board.make_move(mv);

        let fen = board.to_fen();
        let reparsed = Board::from_fen(&fen).unwrap_or_else(|e| panic!("bad FEN {fen}: {e}"));
        assert_eq!(reparsed.to_fen(), fen);
        assert_eq!(reparsed.hash(), board.hash(), "hash mismatch after {uci}: {fen}");
    }
}

fn find_move(board: &Board, uci: &str) -> Option<crate::types::Move> {
    let mut list = MoveList::new();
    board.gen_move(&mut list, false);
    (0..list.len())
        .map(|i| list.get_nth(i))
        .find(|mv| mv.to_string() == uci)
}
