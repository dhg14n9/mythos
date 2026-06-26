use crate::types::Color;

pub struct Score;

impl Score {
    pub const ZERO: i32 = 0;
    pub const MAX: i32 = 50000;
    pub const INF: i32 = 50001;
    pub const NONE: i32 = 50002;

    pub fn score_color(score: i32, color: Color) -> i32 {
        score * match color {
            Color::White => 1,
            Color::Black => -1
        }
    }

    pub fn mate_in(ply: usize, color: Color) -> i32 {
        Self::score_color(Self::MAX - ply as i32, color)
    }
}

