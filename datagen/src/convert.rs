// Replay one game, emit one text line per surviving position.
//
// Output is bulletformat's chess text format (see its README):
//
//     <FEN> | <score> | <result>
//
// with the score in WHITE RELATIVE centipawns and the result WHITE RELATIVE as
// 1.0 / 0.5 / 0.0. The `line=` payload in the PGN is side-to-move relative, so it
// is negated on Black's moves. That negation is the single highest-stakes line in
// this file: an inverted eval trains perfectly well and loses every game.

use std::fmt::Write as _;

use mythos::board::board::Board;
use mythos::types::Color;

use crate::pgn::{self, Game};
use crate::san;

// Above this the score is a mate score, not centipawns — the same bound
// Score::is_mate uses in src/types/score.rs.
const MATE_BOUND: i32 = 40_000;

// Board::state_history is a fixed 1024 entries. The longest game in workload #8
// is 600 plies; bail out rather than overrun it on some future corpus.
const MAX_PLIES: usize = 1000;

pub struct Filters {
    pub min_ply: usize,
    pub max_score: i32,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Counts {
    pub positions: u64,
    pub emitted: u64,
    pub no_score: u64,
    pub mate: u64,
    pub big_score: u64,
    pub noisy: u64,
    pub in_check: u64,
    pub early: u64,
}

impl Counts {
    fn add(&mut self, o: &Counts) {
        self.positions += o.positions;
        self.emitted += o.emitted;
        self.no_score += o.no_score;
        self.mate += o.mate;
        self.big_score += o.big_score;
        self.noisy += o.noisy;
        self.in_check += o.in_check;
        self.early += o.early;
    }
}

#[derive(Default)]
pub struct Stats {
    pub games: u64,
    pub dropped: u64,
    pub counts: Counts,
}

impl Stats {
    pub fn merge_game(&mut self, counts: &Counts) {
        self.counts.add(counts);
    }
}

// Writes the game's lines into `out`, which is cleared first. On Err nothing in
// `out` should be used: a game that does not replay cleanly is dropped whole,
// because every position after the failure would be from the wrong game.
pub fn convert_game(game: &Game, filters: &Filters, out: &mut String) -> Result<Counts, String> {
    out.clear();

    let Some(result) = game.result else {
        return Err("no decisive/drawn result".to_string());
    };
    let mut board = Board::from_fen(&game.fen).map_err(|e| format!("bad FEN: {e}"))?;
    let mut counts = Counts::default();

    for (ply, (token, comment)) in pgn::move_tokens(&game.movetext).enumerate() {
        if ply >= MAX_PLIES {
            return Err(format!("longer than {MAX_PLIES} plies"));
        }

        let mv = san::find_move(&board, token)
            .map_err(|e| format!("`{token}` at ply {ply}: {e}"))?;

        counts.positions += 1;

        // The comment is attached to the move, but describes the position the
        // move is played *from* — the one the board is standing on right now.
        match pgn::comment_score(comment) {
            None => counts.no_score += 1,
            Some(raw) if raw.abs() > MATE_BOUND => counts.mate += 1,
            Some(raw) if raw.abs() > filters.max_score => counts.big_score += 1,
            Some(_) if ply < filters.min_ply => counts.early += 1,
            // A static eval of a position with a hanging piece is noise; that is
            // what qsearch is for.
            Some(_) if mv.is_capture() || mv.is_promotion() => counts.noisy += 1,
            Some(_) if board.is_check() => counts.in_check += 1,
            Some(raw) => {
                let score = if board.stm() == Color::Black { -raw } else { raw };
                let _ = writeln!(out, "{} | {} | {:.1}", board.to_fen(), score, result);
                counts.emitted += 1;
            }
        }

        board.make_move(mv);
    }

    Ok(counts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str) -> (String, Counts) {
        let filters = Filters { min_ply: 0, max_score: 2000 };
        let game = pgn::games(text.as_bytes()).next().unwrap().unwrap();
        let mut out = String::new();
        let counts = convert_game(&game, &filters, &mut out).unwrap();
        (out, counts)
    }

    // The first game of OpenBench workload #8, truncated to three plies.
    const GAME: &str = "\
[Result \"1-0\"]
[FEN \"rnbqkbnr/ppp2p1p/8/3pp1p1/2PP4/N6N/PP2PPPP/R1BQKB1R w KQkq - 0 5\"]
[SetUp \"1\"]

dxe5 {+0.87/4, line=87} Bb4+ {-0.87/3, line=-87} Bd2 {+1.09/6, line=109} 1-0
";

    #[test]
    fn black_scores_are_negated_into_white_relative() {
        let (out, counts) = run(GAME);
        assert_eq!(counts.positions, 3);
        // ply 0 played a capture, ply 2 is White in check after Bb4+.
        assert_eq!(counts.noisy, 1);
        assert_eq!(counts.in_check, 1);
        assert_eq!(counts.emitted, 1);
        // Black was to move and reported -87, so White is +87.
        assert_eq!(
            out,
            "rnbqkbnr/ppp2p1p/8/3pP1p1/2P5/N6N/PP2PPPP/R1BQKB1R b KQkq - 0 5 | 87 | 1.0\n"
        );
    }

    #[test]
    fn mate_scores_and_missing_payloads_are_dropped() {
        let text = "\
[Result \"0-1\"]
[FEN \"4k3/8/8/8/8/8/8/4K3 w - - 0 1\"]

Kd1 {line=49999} Kd8 {unknown} Ke1 {+0.00/4, line=0} 0-1
";
        let (out, counts) = run(text);
        assert_eq!(counts.positions, 3);
        assert_eq!(counts.mate, 1);
        assert_eq!(counts.no_score, 1);
        assert_eq!(counts.emitted, 1);
        assert!(out.ends_with("| 0 | 0.0\n"), "{out}");
    }

    #[test]
    fn a_game_that_does_not_replay_is_an_error() {
        let text = "\
[Result \"1-0\"]
[FEN \"4k3/8/8/8/8/8/8/4K3 w - - 0 1\"]

Qh5 {line=10} 1-0
";
        let game = pgn::games(text.as_bytes()).next().unwrap().unwrap();
        let mut out = String::new();
        let err = convert_game(&game, &Filters { min_ply: 0, max_score: 2000 }, &mut out)
            .unwrap_err();
        assert!(err.contains("Qh5"), "{err}");
    }
}
