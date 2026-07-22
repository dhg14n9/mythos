use std::io::{self, BufRead};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::board::board::Board;
use crate::search::{Search, TimeControl};
use crate::tables::{ThreadData, TransTable};
use crate::types::{Color, Move, MoveList};

const NAME: &str = concat!("Mythos ", env!("CARGO_PKG_VERSION"));
const AUTHOR: &str = "Do Hoang Giang";

const HASH_DEFAULT: usize = 16;
const HASH_MIN: usize = 1;
const HASH_MAX: usize = 4096;

pub fn run() {
    let mut board = Board::start_pos();
    let stop = Arc::new(AtomicBool::new(false));
    let mut hash_mb = HASH_DEFAULT;

    let mut trans_table = TransTable::new(hash_mb);
    let mut thread_data: Option<ThreadData> = Some(ThreadData::new());
    let mut handle: Option<JoinHandle<ThreadData>> = None;

    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let tokens: Vec<&str> = line.split_whitespace().collect();
        let Some((&cmd, args)) = tokens.split_first() else {
            continue;
        };

        match cmd {
            "uci" => {
                println!("id name {NAME}");
                println!("id author {AUTHOR}");
                println!("option name Hash type spin default {HASH_DEFAULT} min {HASH_MIN} max {HASH_MAX}");
                println!("uciok");
            }
            "isready" => println!("readyok"),
            "ucinewgame" => {
                join_thread(&*stop, &mut handle, &mut thread_data);
                board = Board::start_pos();
                trans_table.clear();
                if let Some(td) = thread_data.as_mut() { td.clear() }
            },
            "setoption" => {
                if set_option(args, &mut hash_mb) {
                    join_thread(&*stop, &mut handle, &mut thread_data);
                    trans_table = TransTable::new(hash_mb)
                }
            },
            "position" => position(&mut board, args),
            "go" => go(&mut board, args, &stop, &mut handle, &trans_table, &mut thread_data),
            "bench" => {
                let use_tt = args.iter().any(|a| matches!(*a, "tt" | "--tt"));
                crate::bench::run(use_tt);
            }
            "stop" => { stop.store(true, Ordering::Relaxed) }
            "quit" => {
                stop.store(true, Ordering::Relaxed);
                if let Some(h) = handle.take() {
                    let _ = h.join();
                }
                break;
            },
            _ => println!("info string unknown command: {cmd}"),
        }
    }
}

fn set_option(args: &[&str], hash_mb: &mut usize) -> bool {
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
        Some(n) => println!("info string unknown option: {n}"),
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
    let mut quiet = MoveList::new();
    let mut noisy = MoveList::new();
    board.gen_move(&mut quiet, &mut noisy, false);

    for list in [&quiet, &noisy] {
        for i in 0..list.len() {
            let mv = list.get(i);
            if mv.to_string() == uci {
                return Some(mv);
            }
        }
    }
    None
}

fn go(
    board: &mut Board,
    args: &[&str],
    stop: &Arc<AtomicBool>,
    handle: &mut Option<thread::JoinHandle<ThreadData>>,
    trans_table: &TransTable,
    thread_data: &mut Option<ThreadData>
) {
    let start = Instant::now();

    if let Some((&"perft", rest)) = args.split_first() {
        let depth = rest.first().and_then(|d| d.parse().ok()).unwrap_or(1);
        perft_divide(board, depth);
        return;
    }

    join_thread(stop, handle, thread_data);

    let stop = Arc::clone(stop);
    let (hard_lim, soft_lim) = parse_time(args, board.stm());
    let max_depth = args
        .iter()
        .position(|&a| a == "depth")
        .and_then(|i| args.get(i + 1))
        .and_then(|d| d.parse().ok())
        .unwrap_or(100);
    let mut board = board.clone();
    let tt = trans_table.clone();
    let td = thread_data.take().expect("No thread data");
    *handle = Some(thread::spawn( move || {
        let time_control = TimeControl {
            stop,
            start,
            soft_lim,
            hard_lim
        };
        let mut search = Search::new(time_control, tt, td);
        let best = search.iterative(&mut board, max_depth);
        println!("bestmove {}", best.0);

        search.thread_data
    }));
}

fn join_thread(
    stop: &AtomicBool,
    handle: &mut Option<JoinHandle<ThreadData>>,
    thread_data: &mut Option<ThreadData>
) {
    stop.store(true, Ordering::Relaxed);
    if let Some(h) = handle.take() {
        *thread_data = Some(h.join().unwrap())
    }
    stop.store(false, Ordering::Relaxed);
}

fn perft_divide(board: &mut Board, depth: usize) {
    let start = Instant::now();

    let mut quiet = MoveList::new();
    let mut noisy = MoveList::new();
    board.gen_move(&mut quiet, &mut noisy, false);

    let mut total = 0u64;
    for list in [&quiet, &noisy] {
        for i in 0..list.len() {
            let mv = list.get(i);
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

fn parse_time(args: &[&str], stm: Color) -> (Duration, Duration) {
    // GUI latency
    const OVERHEAD_MS: u64 = 50;

    let value = |key: &str| -> Option<u64> {
        let idx = args.iter().position(|&a| a == key)?;
        args.get(idx + 1)?.parse().ok()
    };

    if let Some(ms) = value("movetime") {
        let lim = Duration::from_millis(ms.saturating_sub(OVERHEAD_MS).max(1));
        return (lim, lim);
    }

    let (time_key, inc_key) = match stm {
        Color::White => ("wtime", "winc"),
        Color::Black => ("btime", "binc"),
    };

    let Some(time) = value(time_key) else {
        return (Duration::MAX, Duration::MAX);
    };

    let time = time.saturating_sub(OVERHEAD_MS).max(1);
    let inc = value(inc_key).unwrap_or(0);
    let mtg = value("movestogo").unwrap_or(25).max(1);

    let hard = (time / 2).max(1);
    let soft = (time / mtg + inc * 3 / 4).clamp(1, hard);

    (Duration::from_millis(hard), Duration::from_millis(soft))
}

