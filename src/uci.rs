use std::io::{self, BufRead};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use crate::board::board::Board;
use crate::search::{Search, TimeControl};
use crate::types::{Color, Move, MoveList};

const NAME: &str = concat!("Mythos ", env!("CARGO_PKG_VERSION"));
const AUTHOR: &str = "Do Hoang Giang";

pub fn run() {
    let mut board = Board::start_pos();
    let stop = Arc::new(AtomicBool::new(false));
    let mut handle: Option<thread::JoinHandle<()>> = None;

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
                println!("uciok");
            }
            "isready" => println!("readyok"),
            "ucinewgame" => board = Board::start_pos(),
            "position" => position(&mut board, args),
            "go" => go(&mut board, args, &stop, &mut handle),
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
    board.gen_move(&mut quiet, &mut noisy);

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
    handle: &mut Option<thread::JoinHandle<()>>
) {
    let start = Instant::now();

    if let Some((&"perft", rest)) = args.split_first() {
        let depth = rest.first().and_then(|d| d.parse().ok()).unwrap_or(1);
        perft_divide(board, depth);
        return;
    }

    stop.store(true, Ordering::Relaxed);
    if let Some(h) = handle.take() {
        let _ = h.join();
    }
    stop.store(false, Ordering::Relaxed);

    let stop = Arc::clone(stop);
    let (hard_lim, soft_lim) = parse_time(args, board.stm());
    let mut board = board.clone();
    *handle = Some(thread::spawn( move || {
        let time_control = TimeControl {
            stop,
            start,
            soft_lim,
            hard_lim
        };
        let mut search = Search::new(time_control);
        let best = search.iterative(&mut board, 100);
        println!("bestmove {}", best.0)
    }));


}

fn perft_divide(board: &mut Board, depth: usize) {
    let start = Instant::now();

    let mut quiet = MoveList::new();
    let mut noisy = MoveList::new();
    board.gen_move(&mut quiet, &mut noisy);

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

