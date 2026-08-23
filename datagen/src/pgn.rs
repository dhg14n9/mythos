// PGN reading, cut down to exactly what the OpenBench DATAGEN output contains.
//
// The upload path rewrites every comment (Client/pgn_util.py), so what arrives is
// COMPACT: the FEN/Result headers, no move numbers, and one comment per move of
// the form `{+0.87/4, line=87}`. `line=` is the engine's own score, printed by the
// `info string pgncomment` in uci.rs.

use std::io::{self, BufRead};

pub struct Game {
    pub fen: String,
    // White relative: 1.0 white win, 0.5 draw, 0.0 black win. None for `*`.
    pub result: Option<f32>,
    pub movetext: String,
}

pub fn games<R: BufRead>(reader: R) -> Games<R> {
    Games { reader, line: String::new(), pending: None, eof: false }
}

pub struct Games<R> {
    reader: R,
    line: String,
    // A header line already read that belongs to the *next* game. Only reachable
    // when a game has no blank line between its movetext and the next header.
    pending: Option<String>,
    eof: bool,
}

struct Partial {
    fen: Option<String>,
    result: Option<f32>,
    movetext: String,
}

impl Partial {
    fn new() -> Self {
        Self { fen: None, result: None, movetext: String::new() }
    }

    fn tag(&mut self, line: &str) {
        let Some((name, value)) = parse_tag(line) else { return };
        match name {
            "FEN" => self.fen = Some(value.to_string()),
            "Result" => self.result = parse_result(value),
            _ => {}
        }
    }

    // A game is only worth emitting once it has movetext; header-only blocks are
    // not games.
    fn finish(self) -> Option<Game> {
        if self.movetext.trim().is_empty() {
            return None;
        }
        Some(Game {
            fen: self.fen.unwrap_or_else(|| START_POS.to_string()),
            result: self.result,
            movetext: self.movetext,
        })
    }
}

const START_POS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

impl<R: BufRead> Iterator for Games<R> {
    type Item = io::Result<Game>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.eof {
            return None;
        }

        let mut game = Partial::new();
        if let Some(line) = self.pending.take() {
            game.tag(&line);
        }

        loop {
            self.line.clear();
            match self.reader.read_line(&mut self.line) {
                Err(e) => {
                    self.eof = true;
                    return Some(Err(e));
                }
                Ok(0) => {
                    self.eof = true;
                    return game.finish().map(Ok);
                }
                Ok(_) => {}
            }

            let line = self.line.trim();

            if line.is_empty() {
                // In PGN a blank line terminates the movetext section. Blank lines
                // before the movetext just separate it from the headers.
                if !game.movetext.is_empty() {
                    if let Some(g) = game.finish() {
                        return Some(Ok(g));
                    }
                    game = Partial::new();
                }
                continue;
            }

            if line.starts_with('[') {
                // A header while we already hold movetext means the previous game
                // was not blank-line terminated. Stash it rather than merging the
                // two games silently.
                if !game.movetext.is_empty() {
                    self.pending = Some(self.line.clone());
                    if let Some(g) = game.finish() {
                        return Some(Ok(g));
                    }
                    game = Partial::new();
                    continue;
                }
                game.tag(line);
            } else {
                game.movetext.push_str(line);
                game.movetext.push(' ');
            }
        }
    }
}

// `[FEN "rnbq..."]` -> ("FEN", "rnbq...")
fn parse_tag(line: &str) -> Option<(&str, &str)> {
    let body = line.strip_prefix('[')?.strip_suffix(']')?;
    let (name, rest) = body.split_once(' ')?;
    let value = rest.trim().strip_prefix('"')?.strip_suffix('"')?;
    Some((name, value))
}

fn parse_result(value: &str) -> Option<f32> {
    match value {
        "1-0" => Some(1.0),
        "0-1" => Some(0.0),
        "1/2-1/2" => Some(0.5),
        _ => None,
    }
}

pub fn move_tokens(movetext: &str) -> MoveTokens<'_> {
    MoveTokens { rest: movetext }
}

// Yields (SAN, comment body without the braces). The comment is "" when a move
// carries none. Move numbers, NAGs and the result token are skipped; anything
// else is handed on as a SAN so that an unrecognised token fails loudly in the
// move matcher instead of being quietly dropped.
pub struct MoveTokens<'a> {
    rest: &'a str,
}

impl<'a> Iterator for MoveTokens<'a> {
    type Item = (&'a str, &'a str);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            self.rest = self.rest.trim_start();
            if self.rest.is_empty() {
                return None;
            }

            // A comment with no move in front of it (fastchess does not emit one,
            // but PGN allows it).
            if self.rest.starts_with('{') {
                let (_, tail) = split_comment(self.rest);
                self.rest = tail;
                continue;
            }

            let end = self
                .rest
                .find(|c: char| c.is_whitespace() || c == '{')
                .unwrap_or(self.rest.len());
            let (token, tail) = self.rest.split_at(end);
            self.rest = tail;

            if !is_move(token) {
                continue;
            }

            let after = self.rest.trim_start();
            if after.starts_with('{') {
                let (comment, tail) = split_comment(after);
                self.rest = tail;
                return Some((token, comment));
            }
            return Some((token, ""));
        }
    }
}

// Input starts at '{'. Returns (body, remainder after '}').
fn split_comment(s: &str) -> (&str, &str) {
    let body = &s[1..];
    match body.find('}') {
        Some(i) => (&body[..i], &body[i + 1..]),
        None => (body, ""),
    }
}

fn is_move(token: &str) -> bool {
    match token {
        "1-0" | "0-1" | "1/2-1/2" | "*" => return false,
        _ => {}
    }
    if token.starts_with('$') {
        return false;
    }
    // Move numbers: "12." / "12..."
    !token.chars().all(|c| c.is_ascii_digit() || c == '.')
}

// Pulls the engine score out of `+0.87/4, line=87`. None when the comment has no
// payload at all — the 23 literal `{unknown}` comments in workload #8.
pub fn comment_score(comment: &str) -> Option<i32> {
    let start = comment.find("line=")? + "line=".len();
    let rest = &comment[start..];
    let end = rest
        .char_indices()
        .position(|(i, c)| !(c.is_ascii_digit() || (c == '-' && i == 0)))
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
[Event \"Fastchess Tournament\"]
[Result \"1-0\"]
[FEN \"rnbqkbnr/ppp2p1p/8/3pp1p1/2PP4/N6N/PP2PPPP/R1BQKB1R w KQkq - 0 5\"]
[SetUp \"1\"]

dxe5 {+0.87/4, line=87} Bb4+ {-0.87/3, line=-87} Bd2 {unknown} 1-0

[Result \"1/2-1/2\"]
[FEN \"4k3/8/8/8/8/8/8/4K3 w - - 0 1\"]

Kd1 {0.00/4, line=0} 1/2-1/2
";

    #[test]
    fn splits_games_and_headers() {
        let parsed: Vec<_> = games(SAMPLE.as_bytes()).map(Result::unwrap).collect();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].result, Some(1.0));
        assert!(parsed[0].fen.starts_with("rnbqkbnr/ppp2p1p"));
        assert_eq!(parsed[1].result, Some(0.5));
    }

    #[test]
    fn pairs_moves_with_comments() {
        let parsed: Vec<_> = games(SAMPLE.as_bytes()).map(Result::unwrap).collect();
        let moves: Vec<_> = move_tokens(&parsed[0].movetext).collect();
        assert_eq!(moves.len(), 3);
        assert_eq!(moves[0].0, "dxe5");
        assert_eq!(comment_score(moves[0].1), Some(87));
        assert_eq!(moves[1].0, "Bb4+");
        assert_eq!(comment_score(moves[1].1), Some(-87));
        // A comment with no payload must still stay attached to its own move.
        assert_eq!(moves[2].0, "Bd2");
        assert_eq!(comment_score(moves[2].1), None);
    }

    #[test]
    fn skips_move_numbers_and_nags() {
        let moves: Vec<_> = move_tokens("1. e4 {line=35} e5 $1 2. Nf3 1/2-1/2").collect();
        assert_eq!(moves.iter().map(|m| m.0).collect::<Vec<_>>(), ["e4", "e5", "Nf3"]);
        assert_eq!(comment_score(moves[0].1), Some(35));
    }
}
