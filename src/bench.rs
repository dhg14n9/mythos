use std::time::Instant;

use crate::board::board::Board;
use crate::search::{Search, TimeControl};
use crate::types::MoveList;

// Andrew Wagner's verified perft suite — 127 positions with known leaf counts
// (http://www.rocechess.ch/perft.html)
const EPD: &str = include_str!("tests/perft_bench.epd");

pub fn cases() -> Vec<(&'static str, usize, u64)> {
    EPD.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|line| {
            let (fen, directive) = line.split_once(';').expect("EPD line missing ';D'");
            let mut fields = directive
                .trim()
                .strip_prefix('D')
                .expect("directive must start with 'D'")
                .split_whitespace();
            let depth = fields
                .next()
                .expect("missing depth")
                .parse()
                .expect("bad depth");
            let expected = fields
                .next()
                .expect("missing count")
                .parse()
                .expect("bad count");
            (fen.trim(), depth, expected)
        })
        .collect()
}

pub fn group_digits(n: u64) -> String {
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

// Search bench: run the real search to a fixed depth over a spread of suite
// positions and report the node count. The total is a functional fingerprint
// of the search — a patch that shouldn't change search behavior must not
// change it.
pub fn search_bench(depth: usize) {
    let cases = cases();
    let positions: Vec<&str> = cases.iter().step_by(6).map(|&(fen, _, _)| fen).collect();

    let mut total_nodes = 0u64;
    let suite_start = Instant::now();

    for (i, fen) in positions.iter().enumerate() {
        let mut board = Board::from_fen(fen).expect("invalid FEN in suite");
        let mut search = Search::new(TimeControl::infinite(), 16);
        search.silent = true;

        let start = Instant::now();
        let (best, _) = search.iterative(&mut board, depth);
        let elapsed = start.elapsed();

        total_nodes += search.nodes;
        let nps = search.nodes as f64 / elapsed.as_secs_f64().max(f64::EPSILON);

        println!(
            "{:>3}/{}  depth {:>2}  nodes {:>13}  time {:>7.3}s  speed {:>7.1} Mnps  bestmove {:<5}  {}",
            i + 1,
            positions.len(),
            depth,
            group_digits(search.nodes),
            elapsed.as_secs_f64(),
            nps / 1e6,
            best.to_string(),
            fen,
        );
    }

    let elapsed = suite_start.elapsed();
    let nps = total_nodes as f64 / elapsed.as_secs_f64().max(f64::EPSILON);

    println!();
    println!("  positions : {}", positions.len());
    println!("  depth     : {depth}");
    println!("  nodes     : {}", group_digits(total_nodes));
    println!("  time      : {:.3?}", elapsed);
    println!(
        "  speed     : {:.1} Mnps ({} nodes/s)",
        nps / 1e6,
        group_digits(nps as u64)
    );
    println!();
    // Machine-friendly last line, same shape OpenBench-style tooling expects.
    println!("Nodes searched: {total_nodes}");
}

// Transposition Table
#[derive(Clone, Copy, Default)]
struct PerftEntry {
    key: u64,
    count: u64,
    depth: u8,
}

pub struct PerftTable {
    entries: Vec<PerftEntry>,
    mask: usize,
    hits: u64,
    probes: u64,
}

impl PerftTable {
    pub fn with_pow2_size(bits: u32) -> Self {
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
        self.entries[(key as usize) & self.mask] = PerftEntry {
            key,
            count,
            depth: depth as u8,
        };
    }

    pub fn hits(&self) -> u64 {
        self.hits
    }

    pub fn probes(&self) -> u64 {
        self.probes
    }
}

pub fn perft_tt(board: &mut Board, depth: usize, tt: &mut PerftTable) -> u64 {
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
    board.gen_move(&mut quiet, &mut noisy, false);

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


pub fn run(use_tt: bool) -> bool {
    let cases = cases();
    
    let mut tt = use_tt.then(|| PerftTable::with_pow2_size(22)); // 2^22 entries, ~96 MB

    if let Some(&(fen, _, _)) = cases.first() {
        let _ = Board::from_fen(fen).unwrap().perft(2);
    }

    let mut total_nodes = 0u64;
    let mut failures = 0usize;
    let suite_start = Instant::now();

    for (i, &(fen, depth, expected)) in cases.iter().enumerate() {
        let mut board = Board::from_fen(fen).expect("invalid FEN in suite");

        let start = Instant::now();
        let nodes = match tt.as_mut() {
            Some(tt) => perft_tt(&mut board, depth, tt),
            None => board.perft(depth),
        };
        let elapsed = start.elapsed();

        total_nodes += nodes;
        let nps = nodes as f64 / elapsed.as_secs_f64().max(f64::EPSILON);

        print!(
            "{:>3}/{}  depth {:>2}  nodes {:>15}  time {:>8.3}s  speed {:>7.1} Mnps  {}",
            i + 1,
            cases.len(),
            depth,
            group_digits(nodes),
            elapsed.as_secs_f64(),
            nps / 1e6,
            fen,
        );
        if nodes == expected {
            println!();
        } else {
            failures += 1;
            println!("  FAIL expected {}", group_digits(expected));
        }
    }

    let elapsed = suite_start.elapsed();
    let nps = total_nodes as f64 / elapsed.as_secs_f64().max(f64::EPSILON);

    println!();
    println!(
        "  positions : {}{}",
        cases.len(),
        if failures == 0 { "" } else { " (FAILURES!)" }
    );
    if failures > 0 {
        println!("  failures  : {failures}");
    }
    println!("  tt        : {}", if use_tt { "on" } else { "off" });
    println!("  nodes     : {}", group_digits(total_nodes));
    println!("  time      : {:.3?}", elapsed);
    println!(
        "  speed     : {:.1} Mnps ({} nodes/s)",
        nps / 1e6,
        group_digits(nps as u64)
    );
    if let Some(tt) = tt.as_ref() {
        let rate = if tt.probes > 0 {
            tt.hits as f64 / tt.probes as f64 * 100.0
        } else {
            0.0
        };
        println!(
            "  tt hits   : {} / {} probes ({rate:.1}%)",
            group_digits(tt.hits),
            group_digits(tt.probes)
        );
    }
    println!();

    failures == 0
}
