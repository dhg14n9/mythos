use std::fs::File;
use std::io::{BufRead, BufReader};

use crate::board::board::Board;
use crate::eval::S;
use crate::eval::eval::{bishop_pair, eval_score, tempo};
use crate::eval::king_safety::king_safety;
use crate::eval::mobility::mobility;
use crate::eval::pawn::pawns;
use crate::eval::piece_square::psqt;
use crate::eval::trace::{
    BISHOP_PAIR, DOUBLED_PAWN, KNIGHT_MOB, NUM_PARAMS, PASSED_PAWN,
    PSQT, QUEEN_MOB, TEMPO, Trace, initial_weights,
};

// The tuner's dataset (gitignored). Used when present; otherwise the fallback list
// below keeps the test meaningful on a fresh clone.
const EPD: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tuner/data/quiet-labeled.epd");
const MAX_POSITIONS: usize = 5000;

#[rustfmt::skip]
const FALLBACK: &[&str] = &[
    "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
    "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
    "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
    "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
    "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
    "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10",
    "4k3/8/8/8/8/8/8/4K3 w - - 0 1",
];

// EPD lines carry only the first 4 FEN fields; from_fen wants 6 and ignores the rest.
fn positions() -> (Vec<String>, &'static str) {
    let Ok(file) = File::open(EPD) else {
        return (FALLBACK.iter().map(|s| s.to_string()).collect(), "fallback FENs");
    };

    let fens = BufReader::new(file)
        .lines()
        .take(MAX_POSITIONS)
        .map(|line| {
            let line = line.expect("read error");
            let fields: Vec<&str> = line.split_whitespace().take(4).collect();
            fields.join(" ") + " 0 1"
        })
        .collect();

    (fens, "dataset")
}

// Sum c_i * w_i over one block of the index space, untapered. Comparing before the
// taper keeps the check exact: tapering is lossy, so an mg error and an equal-and-
// opposite eg error would cancel at some phases and hide a broken coefficient.
fn reconstruct(trace: &Trace, weights: &[S; NUM_PARAMS], block: std::ops::Range<usize>) -> S {
    let coeffs = trace.coeffs();
    let mut score = S(0, 0);

    for i in block {
        if coeffs[i] != 0 {
            score += weights[i] * coeffs[i] as i32;
        }
    }

    score
}

// Each traced term, checked against the eval function it mirrors. Per-term so a
// mismatch names the term that broke.
#[test]
fn trace_terms_match_eval() {
    let weights = initial_weights();
    let (positions, source) = positions();

    for fen in &positions {
        let board = Board::from_fen(fen).unwrap_or_else(|e| panic!("bad FEN {fen}: {e}"));

        let mut trace = Trace::new();
        trace.trace(&board);

        let block = |range: std::ops::Range<usize>| reconstruct(&trace, &weights, range);

        assert_eq!(block(PSQT..PSQT + 384), psqt(&board), "psqt mismatch on {fen}");
        assert_eq!(block(TEMPO..TEMPO + 1), tempo(board.stm()), "tempo mismatch on {fen}");
        assert_eq!(block(BISHOP_PAIR..BISHOP_PAIR + 1), bishop_pair(&board), "bishop_pair mismatch on {fen}");
        assert_eq!(block(PASSED_PAWN..DOUBLED_PAWN + 1), pawns(&board), "pawns mismatch on {fen}");
        assert_eq!(block(KNIGHT_MOB..QUEEN_MOB + 28), mobility(&board), "mobility mismatch on {fen}");

        assert_eq!(trace.frozen(), king_safety(&board), "king safety mismatch on {fen}");
        assert_eq!(trace.phase(), board.phase(), "phase mismatch on {fen}");
    }

    assert!(!positions.is_empty(), "no positions checked");
    eprintln!("checked {} positions ({source})", positions.len());
}

// The whole eval at once. Catches what the per-term checks cannot: a term traced
// into the wrong block still reconstructs correctly per-term if both sides of the
// comparison read the same wrong range.
#[test]
fn trace_total_matches_eval() {
    let weights = initial_weights();
    let (positions, _) = positions();

    for fen in &positions {
        let board = Board::from_fen(fen).unwrap_or_else(|e| panic!("bad FEN {fen}: {e}"));

        let mut trace = Trace::new();
        trace.trace(&board);

        let total = reconstruct(&trace, &weights, 0..NUM_PARAMS) + trace.frozen();

        assert_eq!(total, eval_score(&board), "eval mismatch on {fen}");
    }
}

// Structurally dead params: no legal position can give them a nonzero coefficient,
// so the gradient never reaches them and the tuner must not emit whatever they were
// seeded with. Pawns cannot stand on ranks 1 or 8, so those PSQT entries and the
// matching PASS_PAWN_BONUS ends stay untouched.
#[test]
fn dead_params_never_touched() {
    let (positions, _) = positions();
    let mut seen = [false; NUM_PARAMS];

    for fen in &positions {
        let board = Board::from_fen(fen).unwrap_or_else(|e| panic!("bad FEN {fen}: {e}"));

        let mut trace = Trace::new();
        trace.trace(&board);

        for (i, &c) in trace.coeffs().iter().enumerate() {
            seen[i] |= c != 0;
        }
    }

    for sq in (0..8).chain(56..64) {
        assert!(!seen[PSQT + sq], "pawn psqt rank 1/8 slot {sq} was touched");
    }

    assert!(!seen[PASSED_PAWN], "PASS_PAWN_BONUS[0] was touched");
    assert!(!seen[PASSED_PAWN + 7], "PASS_PAWN_BONUS[7] was touched");
}
