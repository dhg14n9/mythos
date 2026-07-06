use crate::board::board::Board;
use crate::types::{Move, MoveList, PieceType, Square};

/// Count leaf nodes of the move tree at `depth` plies.
///
/// `gen_move` emits fully legal moves, so at `depth == 1` we can bulk-count the
/// generated moves without making them. Both lists must be walked: `noisy` holds
/// captures and queen promotions, `quiet` holds everything else
fn perft(board: &mut Board, depth: usize) -> u64 {
    if depth == 0 {
        return 1;
    }

    let mut quiet = MoveList::new();
    let mut noisy = MoveList::new();
    board.gen_move(&mut quiet, &mut noisy);

    if depth == 1 {
        return (quiet.len() + noisy.len()) as u64;
    }

    let mut count = 0;
    for list in [&quiet, &noisy] {
        for i in 0..list.len() {
            let mv = list.get(i);
            board.make_move(mv);
            count += perft(board, depth - 1);
            board.unmake_move(mv);
        }
    }
    count
}

// ----- perft suite -----

// Each entry is (FEN, leaf counts) where counts[i] is the perft value at depth i+1.
// Source: https://www.chessprogramming.org/Perft_Results
#[rustfmt::skip]
const SUITE: &[(&str, &[u64])] = &[
    // Position 1: initial position
    ("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
     &[20, 400, 8902, 197281, 4865609, 119060324, 3195901860, 84998978956, 2439530234167]),
    // Position 2: Kiwipete
    ("r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
     &[48, 2039, 97862, 4085603, 193690690, 8031647685]),
    // Position 3
    ("8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
     &[14, 191, 2812, 43238, 674624, 11030083, 178633661, 3009794393]),
    // Position 4
    ("r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
     &[6, 264, 9467, 422333, 15833292, 706045033]),
    // Position 4 (mirrored, same results)
    ("r2q1rk1/pP1p2pp/Q4n2/bbp1p3/Np6/1B3NBn/pPPP1PPP/R3K2R b KQ - 0 1",
     &[6, 264, 9467, 422333, 15833292, 706045033]),
    // Position 5
    ("rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
     &[44, 1486, 62379, 2103487, 89941194]),
    // Position 6
    ("r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10",
     &[46, 2079, 89890, 3894594, 164075551, 6923051137, 287188994746]),
];

// Run every suite position to the deepest depth whose expected leaf count stays
// within `max_nodes`. Counts grow monotonically with depth, so once one is over
// budget the rest are too and we stop.
fn run_suite(max_nodes: u64) {
    for &(fen, counts) in SUITE {
        let mut board = Board::from_fen(fen).unwrap_or_else(|e| panic!("bad FEN {fen}: {e}"));
        for (i, &expected) in counts.iter().enumerate() {
            if expected > max_nodes {
                break;
            }
            let depth = i + 1;
            let got = perft(&mut board, depth);
            assert_eq!(got, expected, "perft(depth {depth}) mismatch for FEN `{fen}`");
        }
    }
}

// Fast pass, part of the default suite. Runs each position to the deepest depth
// under ~5M leaf nodes (~17M nodes total).
#[test]
fn perft_suite() {
    run_suite(5_000_000);
}

// Thorough pass, ~800M nodes total. Ignored by default; run with:
//     cargo test perft_suite_deep -- --ignored --nocapture
#[test]
#[ignore]
fn perft_suite_deep() {
    run_suite(200_000_000);
}

// ----- divide (debugging) -----

fn square_to_str(sq: Square) -> String {
    if sq.is_none() {
        return "-".to_string();
    }
    let file = (b'a' + (sq as u8 & 7)) as char;
    let rank = (b'1' + (sq as u8 >> 3)) as char;
    format!("{file}{rank}")
}

fn move_to_uci(mv: Move) -> String {
    let mut s = format!("{}{}", square_to_str(mv.from()), square_to_str(mv.to()));
    if mv.is_promotion() {
        s.push(match mv.promo_piece() {
            PieceType::Knight => 'n',
            PieceType::Bishop => 'b',
            PieceType::Rook => 'r',
            PieceType::Queen => 'q',
            _ => '?',
        });
    }
    s
}

// Print the per-root-move node counts, the standard way to bisect a perft
// discrepancy against a reference engine (e.g. `stockfish`'s `go perft N`).
fn perft_divide(board: &mut Board, depth: usize) -> u64 {
    let mut quiet = MoveList::new();
    let mut noisy = MoveList::new();
    board.gen_move(&mut quiet, &mut noisy);

    let mut total = 0;
    for list in [&quiet, &noisy] {
        for i in 0..list.len() {
            let mv = list.get(i);
            board.make_move(mv);
            let sub = if depth <= 1 { 1 } else { perft(board, depth - 1) };
            board.unmake_move(mv);
            println!("{}: {sub}", move_to_uci(mv));
            total += sub;
        }
    }
    println!("\nNodes searched: {total}");
    total
}

// Edit the FEN/depth and run to bisect a failing position:
//     cargo test perft_debug -- --ignored --nocapture
#[test]
#[ignore]
fn perft_debug() {
    let fen = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";
    let depth = 1;
    let mut board = Board::from_fen(fen).unwrap();
    perft_divide(&mut board, depth);
}
