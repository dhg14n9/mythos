use mythos::board::board::Board;
use std::fs::File;
use std::io::{BufRead, BufReader};

pub fn parse_data(path: &str, limit: Option<usize>, mut f: impl FnMut(&Board, f32)) {

    let file = File::open(path);
    if let Err(e) = file {
        panic!("Can't open {path}: {e}")
    }
    let file = file.unwrap();

    for (i, s) in BufReader::new(file).lines().take(limit.unwrap_or(usize::MAX)).enumerate() {
        let s = s.unwrap();

        if s.trim().is_empty() {
            continue
        }

        let elements: Vec<&str> = s.split_whitespace().collect();
        let fen = elements[0..4].join(" ") + " 0 1";
        let score: f32 = match s.split('"').nth(1).unwrap_or_else(|| panic!("line {i}: No Label")) {
            "1-0" => {1.0},
            "1/2-1/2" => {0.5},
            "0-1" => {0.0},
            _ => panic!("line {i}: Invalid Score")

        };
        let board = Board::from_fen(&*fen).unwrap_or_else(|e| panic!("line {i}: {e}"));

        f(&board, score)
    }



}