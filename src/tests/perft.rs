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

// Divide a position to bisect a perft discrepancy. FEN and depth come from the
// PERFT_FEN / PERFT_DEPTH env vars (Kiwipete at depth 1 by default), so no recompile
// is needed to point it somewhere new:
//     PERFT_FEN="<fen>" PERFT_DEPTH=3 cargo test perft_debug -- --ignored --nocapture
#[test]
#[ignore]
fn perft_debug() {
    let fen = std::env::var("PERFT_FEN")
        .unwrap_or_else(|_| "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1".to_string());
    let depth = std::env::var("PERFT_DEPTH")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let mut board = Board::from_fen(&fen).expect("PERFT_FEN is not a valid FEN");
    perft_divide(&mut board, depth);
}

// ----- transposition table (perft hashing) -----

// A perft subtree count depends only on (position, remaining depth), so it can be
// cached. Keyed by the board's zobrist hash; `depth` is stored and compared so
// entries for different depths of the same position never alias. Direct-mapped
// (one slot per index, newest wins) — collisions only cost a recomputation.
#[derive(Clone, Copy, Default)]
struct PerftEntry {
    key: u64,
    count: u64,
    depth: u8,
}

struct PerftTable {
    entries: Vec<PerftEntry>,
    mask: usize,
    hits: u64,
    probes: u64,
}

impl PerftTable {
    fn with_pow2_size(bits: u32) -> Self {
        let size = 1usize << bits;
        Self {
            entries: vec![PerftEntry::default(); size],
            mask: size - 1,
            hits: 0,
            probes: 0,
        }
    }

    fn probe(&mut self, key: u64, depth: usize) -> Option<u64> {
        self.probes += 1;
        let e = &self.entries[(key as usize) & self.mask];
        if e.key == key && e.depth as usize == depth {
            self.hits += 1;
            Some(e.count)
        } else {
            None
        }
    }

    fn store(&mut self, key: u64, depth: usize, count: u64) {
        self.entries[(key as usize) & self.mask] = PerftEntry { key, count, depth: depth as u8 };
    }
}

// perft with transposition-table lookups. Probes before generating moves so a hit
// skips movegen entirely. depth 0/1 are trivial and never cached.
fn perft_tt(board: &mut Board, depth: usize, tt: &mut PerftTable) -> u64 {
    if depth == 0 {
        return 1;
    }

    let key = board.hash();
    if depth >= 2 {
        if let Some(count) = tt.probe(key, depth) {
            return count;
        }
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
            count += perft_tt(board, depth - 1, tt);
            board.unmake_move(mv);
        }
    }
    tt.store(key, depth, count);
    count
}

// ----- benchmark -----

// Insert thousands separators: 119060324 -> "119,060,324".
fn group_digits(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    let len = s.len();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

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
    let _ = perft(&mut board, 2.min(depth));

    // With the TT on, `nodes` is still the true leaf count, so `speed` is the
    // *effective* throughput (full node count / reduced time) — the number to
    // compare against the TT-off run.
    let (nodes, elapsed, tt_stats) = if use_tt {
        let mut tt = PerftTable::with_pow2_size(22); // 2^22 entries, ~96 MB
        let start = Instant::now();
        let n = perft_tt(&mut board, depth, &mut tt);
        (n, start.elapsed(), Some((tt.hits, tt.probes)))
    } else {
        let start = Instant::now();
        let n = perft(&mut board, depth);
        (n, start.elapsed(), None)
    };

    let secs = elapsed.as_secs_f64();
    let nps = if secs > 0.0 { nodes as f64 / secs } else { f64::INFINITY };

    println!();
    println!("  position : {fen}");
    println!("  depth    : {depth}");
    println!("  tt       : {}", if use_tt { "on" } else { "off" });
    println!("  nodes    : {}", group_digits(nodes));
    println!("  time     : {:.3?}", elapsed);
    println!("  speed    : {:.1} Mnps ({} nodes/s)", nps / 1e6, group_digits(nps as u64));
    if let Some((hits, probes)) = tt_stats {
        let rate = if probes > 0 { hits as f64 / probes as f64 * 100.0 } else { 0.0 };
        println!("  tt hits  : {} / {} probes ({rate:.1}%)", group_digits(hits), group_digits(probes));
    }
    println!();
}

// Andrew Wagner's verified perft suite — 127 positions with known leaf counts,
// embedded from perft_bench.epd (http://www.rocechess.ch/perft.html). Times one
// full pass (~4.7B nodes) and reports aggregate throughput. Every position's count
// is asserted against the file's verified value, so this is also a broad
// correctness sweep across castling, promotions, en passant, and endgames.
// The transposition table is toggled with PERFT_TT (off by default). Run with:
//     PERFT_TT=1 cargo test perft_bench_suite -- --ignored --nocapture
#[test]
#[ignore]
fn perft_bench_suite() {
    use std::time::Instant;

    // Embedded at compile time; lives next to this file.
    const EPD: &str = include_str!("perft_bench.epd");

    // Parse "<FEN> ;D<depth> <expected>" lines, skipping comments and blanks.
    let cases: Vec<(&str, usize, u64)> = EPD
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|line| {
            let (fen, directive) = line.split_once(';').expect("EPD line missing ';D'");
            let mut fields = directive
                .trim()
                .strip_prefix('D')
                .expect("directive must start with 'D'")
                .split_whitespace();
            let depth = fields.next().expect("missing depth").parse().expect("bad depth");
            let expected = fields.next().expect("missing count").parse().expect("bad count");
            (fen.trim(), depth, expected)
        })
        .collect();

    let use_tt = tt_enabled();
    // One table shared across the whole pass. Full 64-bit key + depth comparison
    // keeps positions independent (a different position can never read another's
    // entry), so there is no need to clear between positions.
    let mut tt = use_tt.then(|| PerftTable::with_pow2_size(22));

    // Warm the hot paths before timing.
    if let Some(&(fen, _, _)) = cases.first() {
        let _ = perft(&mut Board::from_fen(fen).unwrap(), 2);
    }

    let mut total_nodes = 0u64;
    let start = Instant::now();
    for &(fen, depth, expected) in &cases {
        let mut board = Board::from_fen(fen).expect("invalid FEN in suite");
        let nodes = match tt.as_mut() {
            Some(tt) => perft_tt(&mut board, depth, tt),
            None => perft(&mut board, depth),
        };
        assert_eq!(nodes, expected, "perft(depth {depth}) mismatch for `{fen}`");
        total_nodes += nodes;
    }
    let elapsed = start.elapsed();

    let secs = elapsed.as_secs_f64();
    let nps = if secs > 0.0 { total_nodes as f64 / secs } else { f64::INFINITY };

    println!();
    println!("  suite     : Andrew Wagner (perft_bench.epd)");
    println!("  positions : {}", cases.len());
    println!("  tt        : {}", if use_tt { "on" } else { "off" });
    println!("  nodes     : {}", group_digits(total_nodes));
    println!("  time      : {:.3?}", elapsed);
    println!("  speed     : {:.1} Mnps ({} nodes/s)", nps / 1e6, group_digits(nps as u64));
    if let Some(tt) = tt.as_ref() {
        let rate = if tt.probes > 0 { tt.hits as f64 / tt.probes as f64 * 100.0 } else { 0.0 };
        println!("  tt hits   : {} / {} probes ({rate:.1}%)", group_digits(tt.hits), group_digits(tt.probes));
    }
    println!();
}
