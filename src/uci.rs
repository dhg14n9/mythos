use std::io::{self, BufRead};
use std::time::Instant;

use crate::board::board::Board;
use crate::movepicker::MovePicker;
use crate::types::{Move, MoveList};

const NAME: &str = concat!("Mythos ", env!("CARGO_PKG_VERSION"));
const AUTHOR: &str = "Do Hoang Giang";

/// Read UCI commands from stdin until `quit` or EOF.
///
/// Everything runs on this one thread: `go` returns its bestmove before the
/// next command is read, so `stop` has nothing to interrupt yet. Once a real
/// search exists, `go` moves to a worker thread and `stop` signals it.
pub fn run() {
    let mut board = Board::start_pos();

    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let tokens: Vec<&str> = line.split_whitespace().collect();
        let Some((&cmd, args)) = tokens.split_first() else { continue };

        match cmd {
            "uci" => {
                println!("id name {NAME}");
                println!("id author {AUTHOR}");
                println!("uciok");
            }
            "isready" => println!("readyok"),
            "ucinewgame" => board = Board::start_pos(),
            "position" => position(&mut board, args),
            "go" => go(&mut board, args),
            "bench" => {
                let use_tt = args.iter().any(|a| matches!(*a, "tt" | "--tt"));
                crate::bench::run(use_tt);
            }
            "stop" => {} // nothing running; bestmove was already sent
            "quit" => break,
            _ => println!("info string unknown command: {cmd}"),
        }
    }
}

// `position startpos [moves ...]` or `position fen <fen> [moves ...]`.
// On any malformed input the current position is left untouched.
fn position(board: &mut Board, args: &[&str]) {
    let (new_board, rest) = match args.split_first() {
        Some((&"startpos", rest)) => (Board::start_pos(), rest),
        Some((&"fen", rest)) => {
            let fen_end = rest.iter().position(|&t| t == "moves").unwrap_or(rest.len());
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

// Resolve a UCI move string against the legal moves of `board`. Matching the
// generated moves (rather than parsing squares directly) both validates the
// move and recovers its MoveKind for free.
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

fn go(board: &mut Board, args: &[&str]) {
    if let Some((&"perft", rest)) = args.split_first() {
        let depth = rest.first().and_then(|d| d.parse().ok()).unwrap_or(1);
        perft_divide(board, depth);
        return;
    }

    // No search yet: time controls are ignored and the move picker chooses a
    // legal move (deterministically pseudo-random from the position hash).
    // "bestmove 0000" signals a position with no legal moves.
    let mut picker = MovePicker::new(board);
    picker.gen_move();
    println!("bestmove {}", picker.random());
}

// `go perft <depth>`: print per-root-move subtree counts (a "divide"), the
// standard way to bisect a movegen discrepancy against a reference engine.
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
            let count = if depth <= 1 { 1 } else { board.perft(depth - 1) };
            board.unmake_move(mv);
            println!("{mv}: {count}");
            total += count;
        }
    }

    let elapsed = start.elapsed();
    let nps = total as f64 / elapsed.as_secs_f64().max(f64::EPSILON);
    println!();
    println!("info string perft({depth}) time {} ms nps {}", elapsed.as_millis(), nps as u64);
    println!("Nodes searched: {total}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{MoveKind, Square};

    #[test]
    fn move_uci_notation() {
        assert_eq!(Move::new(Square::E2, Square::E4, MoveKind::DoublePush).to_string(), "e2e4");
        assert_eq!(Move::new(Square::E7, Square::E8, MoveKind::PromoQueen).to_string(), "e7e8q");
        assert_eq!(Move::new(Square::B7, Square::A8, MoveKind::CapPromoKnight).to_string(), "b7a8n");
        assert_eq!(Move::default().to_string(), "0000");
    }

    #[test]
    fn find_move_recovers_kind() {
        let board = Board::start_pos();
        let mv = find_move(&board, "e2e4").expect("e2e4 is legal from startpos");
        assert!(mv.is_double_push());
        assert!(find_move(&board, "e2e5").is_none());
    }

    // Replaying a game must reach the same position as parsing its FEN directly.
    #[test]
    fn position_moves_match_fen() {
        let mut replayed = Board::start_pos();
        position(&mut replayed, &["startpos", "moves", "e2e4", "c7c5", "g1f3"]);

        let direct =
            Board::from_fen("rnbqkbnr/pp1ppppp/8/2p5/4P3/5N2/PPPP1PPP/RNBQKB1R b KQkq - 1 3")
                .unwrap();
        assert_eq!(replayed.hash(), direct.hash());
    }

    // An illegal move must leave the previous position untouched.
    #[test]
    fn position_rejects_illegal_move_atomically() {
        let mut board = Board::start_pos();
        position(&mut board, &["startpos", "moves", "e2e4"]);
        let before = board.hash();

        position(&mut board, &["startpos", "moves", "e2e4", "e7e6", "e4e6"]);
        assert_eq!(board.hash(), before);
    }
}