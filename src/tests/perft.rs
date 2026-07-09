use crate::board::board::Board;

// The plain perft implementation lives on Board (src/board/movegen.rs) so the
// UCI `go perft` command shares it; only the TT-assisted variant is test-local.

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
            let got = board.perft(depth);
            assert_eq!(
                got, expected,
                "perft(depth {depth}) mismatch for FEN `{fen}`"
            );
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

// A per-root-move divide is available through the UCI loop (`go perft <depth>`),
// which is the standard way to bisect a discrepancy against a reference engine:
//     echo "position fen <fen>\ngo perft 3" | cargo run --release

// ----- benchmark -----

// The transposition-table perft (PerftTable / perft_tt) lives in crate::bench
// alongside the suite runner, so the `mythos bench tt` CLI command shares it.
use crate::bench::{PerftTable, group_digits, perft_tt};

// Whether the PERFT_TT env var opts into the transposition table.
fn tt_enabled() -> bool {
    std::env::var("PERFT_TT")
        .map(|v| matches!(v.as_str(), "1" | "true" | "on" | "yes"))
        .unwrap_or(false)
}

// Time a single perft and report throughput. Controlled by env vars:
//   PERFT_FEN    position to search      (start position by default)
//   PERFT_DEPTH  depth in plies          (6 by default — ~119M nodes)
//   PERFT_TT     1/true/on to enable the transposition table (off by default)
// Run with:
//     PERFT_DEPTH=7 PERFT_TT=1 cargo test perft_bench -- --ignored --nocapture
#[test]
#[ignore]
fn perft_bench() {
    use std::time::Instant;

    let fen = std::env::var("PERFT_FEN")
        .unwrap_or_else(|_| "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1".to_string());
    let depth = std::env::var("PERFT_DEPTH")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(6);
    let use_tt = tt_enabled();

    let mut board = Board::from_fen(&fen).expect("PERFT_FEN is not a valid FEN");

    // Touch the hot paths once so the timed run reflects steady-state throughput
    // (I-cache / branch predictor warmed) rather than first-call effects.
    let _ = board.perft(2.min(depth));

    // With the TT on, `nodes` is still the true leaf count, so `speed` is the
    // *effective* throughput (full node count / reduced time) — the number to
    // compare against the TT-off run.
    let (nodes, elapsed, tt_stats) = if use_tt {
        let mut tt = PerftTable::with_pow2_size(22); // 2^22 entries, ~96 MB
        let start = Instant::now();
        let n = perft_tt(&mut board, depth, &mut tt);
        (n, start.elapsed(), Some((tt.hits(), tt.probes())))
    } else {
        let start = Instant::now();
        let n = board.perft(depth);
        (n, start.elapsed(), None)
    };

    let secs = elapsed.as_secs_f64();
    let nps = if secs > 0.0 {
        nodes as f64 / secs
    } else {
        f64::INFINITY
    };

    println!();
    println!("  position : {fen}");
    println!("  depth    : {depth}");
    println!("  tt       : {}", if use_tt { "on" } else { "off" });
    println!("  nodes    : {}", group_digits(nodes));
    println!("  time     : {:.3?}", elapsed);
    println!(
        "  speed    : {:.1} Mnps ({} nodes/s)",
        nps / 1e6,
        group_digits(nps as u64)
    );
    if let Some((hits, probes)) = tt_stats {
        let rate = if probes > 0 {
            hits as f64 / probes as f64 * 100.0
        } else {
            0.0
        };
        println!(
            "  tt hits  : {} / {} probes ({rate:.1}%)",
            group_digits(hits),
            group_digits(probes)
        );
    }
    println!();
}

// Andrew Wagner's verified suite (127 positions, ~4.7B nodes), shared with the
// `mythos bench` CLI command. Prints per-position nodes/time/NPS and overall
// totals; the assert makes any count mismatch fail the test.
// The transposition table is toggled with PERFT_TT (off by default). Run with:
//     PERFT_TT=1 cargo test perft_bench_suite -- --ignored --nocapture
#[test]
#[ignore]
fn perft_bench_suite() {
    assert!(
        crate::bench::run(tt_enabled()),
        "perft bench suite had count mismatches"
    );
}
