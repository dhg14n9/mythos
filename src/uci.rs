use std::io::{self, BufRead};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::board::board::Board;
use crate::search::{NodeCounter, Search, TimeControl};
use crate::tables::{ThreadData, TransTable};
use crate::types::{Color, Move, MoveList, Rng};

const NAME: &str = concat!("Mythos ", env!("CARGO_PKG_VERSION"));
const AUTHOR: &str = "Do Hoang Giang";

const HASH_DEFAULT: usize = 16;
const HASH_MIN: usize = 1;
const HASH_MAX: usize = 4096;

const THREAD_MIN: usize = 1;
const THREAD_MAX: usize = 256;

struct Session {
    board: Board,
    stop: Arc<AtomicBool>,
    hash_mb: usize,
    thread: usize,
    trans_table: TransTable,
    thread_data: Vec<ThreadData>,
    handle: Option<JoinHandle<Vec<ThreadData>>>
}

impl Session {
    pub fn new(hash_mb: usize) -> Self {
        Self {
            board: Board::start_pos(),
            stop: Arc::new(AtomicBool::new(false)),
            hash_mb,
            thread: THREAD_MIN,
            trans_table: TransTable::new(hash_mb),
            thread_data: vec![ThreadData::new()],
            handle: None
        }
    }

    pub fn execute(&mut self, line: &str) -> Flow {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        let Some((&cmd, args)) = tokens.split_first() else {
            return Flow::Continue;
        };

        match cmd {
            "uci" => {
                println!("id name {NAME}");
                println!("id author {AUTHOR}");
                println!("option name Hash type spin default {HASH_DEFAULT} min {HASH_MIN} max {HASH_MAX}");
                println!("option name Threads type spin default {THREAD_MIN} min {THREAD_MIN} max {THREAD_MAX}");
                crate::tunables::print_options();
                println!("uciok");
            }
            "isready" => println!("readyok"),
            "ucinewgame" => {
                join_thread(&*self.stop, &mut self.handle, &mut self.thread_data);
                self.board = Board::start_pos();
                self.trans_table.clear();
                for td in &mut self.thread_data {td.clear()}
            },
            "setoption" => {
                let rebuild_tt = set_option(args, &mut self.hash_mb, &mut self.thread);
                if rebuild_tt || self.thread != self.thread_data.len() {
                    join_thread(&*self.stop, &mut self.handle, &mut self.thread_data);
                    if rebuild_tt { self.trans_table = TransTable::new(self.hash_mb) }
                    self.thread_data.resize_with(self.thread, ThreadData::new);
                }
            },
            "position" => position(&mut self.board, args),
            "go" => go(&mut self.board, args, &self.stop, &mut self.handle, &mut self.trans_table, &mut self.thread_data),
            "bench" => {
                let (depth, hash) = crate::bench::parse_bench_args(args);
                crate::bench::search_bench(depth, hash);
            }
            // Non-standard: dumps the OpenBench SPSA block
            "spsa" => crate::tunables::print_spsa(),
            "eval" => crate::nnue::trace::print(&self.board),
            // Non-standard: OpenBench DATAGEN opening generation.
            "genfens" => genfens(args),
            "perftsuite" => {
                let use_tt = args.iter().any(|a| matches!(*a, "tt" | "--tt"));
                crate::bench::run(use_tt);
            }
            "stop" => { self.stop.store(true, Ordering::Relaxed) }
            "quit" => {
                self.stop.store(true, Ordering::Relaxed);
                if let Some(h) = self.handle.take() {
                    let _ = h.join();
                }
                return Flow::Quit
            },
            _ => println!("info string unknown command: {cmd}"),
        }
        Flow::Continue

    }

}

enum Flow {
    Continue, Quit
}

pub fn run(args: &[String]) {
    let mut session = Session::new(HASH_DEFAULT);

    for arg in args {
        if let Flow::Quit = session.execute(arg) {
            return;
        }
    }

    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        match session.execute(&line) {
            Flow::Continue => {}
            Flow::Quit => break,
        }
    }
}

const GENFENS_PLIES: usize = 8;
const GENFENS_DEPTH: usize = 6;
const GENFENS_CUTOFF: i32 = 400;
const GENFENS_TRIES: usize = 100;
const GENFENS_HASH: usize = 1;

fn genfens(args: &[&str]) {
    let count = args.first().and_then(|n| n.parse().ok()).unwrap_or(1);
    let seed = keyword(args, "seed").and_then(|s| s.parse().ok()).unwrap_or(0);
    let book = keyword(args, "book").filter(|&b| b != "None");

    gen_openings(count, seed, book, |fen| println!("info string genfens {fen}"));
}

fn keyword<'a>(args: &[&'a str], key: &str) -> Option<&'a str> {
    args.iter()
        .position(|&a| a == key)
        .and_then(|i| args.get(i + 1))
        .copied()
}

pub(crate) fn gen_openings(count: usize, seed: u64, book: Option<&str>, mut emit: impl FnMut(&str)) {
    if let Some(path) = book {
        println!("info string genfens: book {path} is not supported, using startpos");
    }

    let mut rng = Rng::new(seed);
    let trans_table = TransTable::new(GENFENS_HASH);
    let mut thread_data = ThreadData::new();

    for _ in 0..count {
        let board;
        (board, thread_data) = random_opening(&mut rng, &trans_table, thread_data, GENFENS_PLIES);
        if let Some(board) = board {
            emit(&board.to_fen());
        }
    }
}

pub(crate) fn random_opening(
    rng: &mut Rng,
    trans_table: &TransTable,
    mut thread_data: ThreadData,
    base_plies: usize
) -> (Option<Board>, ThreadData) {
    let start = Board::start_pos();

    for tries in 0..GENFENS_TRIES {
        let plies = base_plies + rng.next_below(2) as usize;
        let Some(mut board) = random_line(rng, &start, plies) else {
            continue;
        };

        let mut list = MoveList::new();
        board.gen_move(&mut list, false);
        if list.len() == 0 || board.is_draw() {
            continue;
        }

        let mut search = Search::new(TimeControl::infinite(), trans_table.clone(), thread_data);
        search.silent = true;
        let (_, score) = search.iterative(&mut board, GENFENS_DEPTH);
        thread_data = search.thread_data;

        if score.abs() > GENFENS_CUTOFF && tries + 1 < GENFENS_TRIES {
            continue;
        }

        return (Some(board), thread_data);
    }

    (None, thread_data)
}

fn random_line(rng: &mut Rng, start: &Board, plies: usize) -> Option<Board> {
    let mut board = start.clone();
    let mut list = MoveList::new();

    for _ in 0..plies {
        list.clear();
        board.gen_move(&mut list, false);
        if list.len() == 0 {
            return None;
        }
        let index = rng.next_below(list.len() as u64) as usize;
        board.make_move(list.get_nth(index));
    }

    Some(board)
}

fn set_option(args: &[&str], hash_mb: &mut usize, thread: &mut usize) -> bool {
    // setoption name <id> value <x>
    let name = args
        .iter()
        .position(|&a| a == "name")
        .and_then(|i| args.get(i + 1))
        .copied();
    let value = args
        .iter()
        .position(|&a| a == "value")
        .and_then(|i| args.get(i + 1));

    match name {
        Some(n) if n.eq_ignore_ascii_case("hash") => {
            match value.and_then(|v| v.parse::<usize>().ok()) {
                Some(mb) => {
                    if mb != *hash_mb {
                        *hash_mb = mb.clamp(HASH_MIN, HASH_MAX);
                        return true;
                    }
                },
                None => println!("info string invalid value for Hash"),
            }
        }
        Some(n) if n.eq_ignore_ascii_case("threads") => {
            match value.and_then(|v| v.parse::<usize>().ok()) {
                Some(t) if (THREAD_MIN..=THREAD_MAX).contains(&t) => *thread = t,
                Some(t) => println!("Threads {t} out of range [{THREAD_MIN}, {THREAD_MAX}], staying at {thread}"),
                None => println!("info string invalid value for Threads"),
            }
        }
        // see tunables.rs
        Some(n) => {
            if !crate::tunables::set(n, value.copied().unwrap_or("")) {
                println!("info string unknown option: {n}")
            }
        }
        None => println!("info string malformed setoption"),
    }
    return false;
}

fn position(board: &mut Board, args: &[&str]) {
    let (new_board, rest) = match args.split_first() {
        Some((&"startpos", rest)) => (Board::start_pos(), rest),
        Some((&"fen", rest)) => {
            let fen_end = rest
                .iter()
                .position(|&t| t == "moves")
                .unwrap_or(rest.len());
            match Board::from_fen(&rest[..fen_end].join(" ")) {
                Ok(b) => (b, &rest[fen_end..]),
                Err(e) => {
                    println!("info string invalid fen: {e}");
                    return;
                }
            }
        }
        _ => {
            println!("info string malformed position command");
            return;
        }
    };

    let mut new_board = new_board;
    if let Some((&"moves", moves)) = rest.split_first() {
        for &token in moves {
            let Some(mv) = find_move(&new_board, token) else {
                println!("info string illegal move: {token}");
                return;
            };
            new_board.make_move(mv);
        }
    }
    *board = new_board;
}

fn find_move(board: &Board, uci: &str) -> Option<Move> {
    let mut list = MoveList::new();
    board.gen_move(&mut list, false);

    for i in 0..list.len() {
        let mv = list.get_nth(i);
        if mv.to_string() == uci {
            return Some(mv);
        }
    }
    None
}

fn go(
    board: &mut Board,
    args: &[&str],
    stop: &Arc<AtomicBool>,
    handle: &mut Option<JoinHandle<Vec<ThreadData>>>,
    trans_table: &mut TransTable,
    thread_data: &mut Vec<ThreadData>
) {
    let start = Instant::now();

    if let Some((&"perft", rest)) = args.split_first() {
        let depth = rest.first().and_then(|d| d.parse().ok()).unwrap_or(1);
        perft_divide(board, depth);
        return;
    }

    join_thread(stop, handle, thread_data);
    trans_table.generation = (trans_table.generation + 1) & TransTable::AGE_MASK; 

    let stop = Arc::clone(stop);
    let (hard_lim, soft_lim) = parse_time(args, board.stm());
    let max_depth = value(args, "depth").unwrap_or(100) as usize;
    let hard_node = value(args, "nodes").unwrap_or(u64::MAX);
    let mut board = board.clone();
    let tt = trans_table.clone();
    let tds = std::mem::take(thread_data);
    *handle = Some(thread::spawn(move || {
        let threads = tds.len();
        let shared_nodes = Arc::new(NodeCounter::new(threads));
        let mut tds = tds.into_iter();
        let main_td = tds.next().expect("no thread data");

        thread::scope(|s| {
            // Helpers: no time or node limits, they run until the main thread raises `stop`.
            let helpers: Vec<_> = tds.enumerate().map(|(i, td)| {
                let time_control = TimeControl {
                    stop: Arc::clone(&stop),
                    start,
                    soft_lim: Duration::MAX,
                    hard_lim: Duration::MAX,
                    soft_base: Duration::MAX,
                    hard_node: u64::MAX,
                    soft_node: u64::MAX
                };
                let tt = tt.clone();
                let shared_nodes = Arc::clone(&shared_nodes);
                let mut board = board.clone();
                s.spawn(move || {
                    let mut search = Search::new(time_control, tt, td);
                    search.silent = true;
                    search.thread_id = i + 1;
                    search.shared_nodes = shared_nodes;
                    search.iterative(&mut board, max_depth);
                    search.thread_data
                })
            }).collect();

            let time_control = TimeControl {
                stop: Arc::clone(&stop),
                start,
                soft_lim,
                hard_lim,
                soft_base: soft_lim,
                hard_node,
                soft_node: u64::MAX
            };
            let mut search = Search::new(time_control, tt.clone(), main_td);
            search.shared_nodes = Arc::clone(&shared_nodes);
            let best = search.iterative(&mut board, max_depth);

            stop.store(true, Ordering::Relaxed);

            #[cfg(feature = "datagen")]
            println!("info string pgncomment {}", best.1);
            println!("bestmove {}", best.0);

            let mut out = Vec::with_capacity(threads);
            out.push(search.thread_data);
            out.extend(helpers.into_iter().map(|h| h.join().unwrap()));
            out
        })
    }));
}

fn join_thread(
    stop: &AtomicBool,
    handle: &mut Option<JoinHandle<Vec<ThreadData>>>,
    thread_data: &mut Vec<ThreadData>
) {
    stop.store(true, Ordering::Relaxed);
    if let Some(h) = handle.take() {
        *thread_data = h.join().unwrap()
    }
    stop.store(false, Ordering::Relaxed);
}

fn perft_divide(board: &mut Board, depth: usize) {
    let start = Instant::now();

    let mut list = MoveList::new();
    board.gen_move(&mut list, false);

    let mut total = 0u64;
    for i in 0..list.len() {
        let mv = list.get_nth(i);
        board.make_move(mv);
        let count = if depth <= 1 {
            1
        } else {
            board.perft(depth - 1)
        };
        board.unmake_move(mv);
        println!("{mv}: {count}");
        total += count;
    }

    let elapsed = start.elapsed();
    let nps = total as f64 / elapsed.as_secs_f64().max(f64::EPSILON);
    println!();
    println!(
        "info string perft({depth}) time {} ms nps {}",
        elapsed.as_millis(),
        nps as u64
    );
    println!("Nodes searched: {total}");
}

// value of a `go` parameter, e.g. `nodes 5000` -> value(args, "nodes") == Some(5000)
fn value(args: &[&str], key: &str) -> Option<u64> {
    let idx = args.iter().position(|&a| a == key)?;
    args.get(idx + 1)?.parse().ok()
}

fn parse_time(args: &[&str], stm: Color) -> (Duration, Duration) {
    // GUI latency
    const OVERHEAD_MS: u64 = 50;

    if let Some(ms) = value(args, "movetime") {
        let lim = Duration::from_millis(ms.saturating_sub(OVERHEAD_MS).max(1));
        return (lim, lim);
    }

    let (time_key, inc_key) = match stm {
        Color::White => ("wtime", "winc"),
        Color::Black => ("btime", "binc"),
    };

    let Some(time) = value(args, time_key) else {
        return (Duration::MAX, Duration::MAX);
    };

    let time = time.saturating_sub(OVERHEAD_MS).max(1);
    let inc = value(args, inc_key).unwrap_or(0);
    let mtg = value(args, "movestogo").unwrap_or(40).max(1);

    let hard = (time / 4).max(1);
    let soft = (time / mtg + inc * 1 / 4).clamp(1, hard);

    (Duration::from_millis(hard), Duration::from_millis(soft))
}

