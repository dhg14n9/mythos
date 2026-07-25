use mythos::board::board::Board;
use std::fs;

pub fn parse_data(path: &str, mut f: impl FnMut(&Board, f32)) {
    let content = fs::read_to_string(path).expect("Cannot read data");

    for (i, s) in content.lines().enumerate() {
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