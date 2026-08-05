use crate::types::Color;

pub struct Score;

impl Score {
    pub const ZERO: i32 = 0;
    pub const MAX: i32 = 50000;
    pub const INF: i32 = 50001;
    pub const NONE: i32 = 50002;

    pub fn score_color(score: i32, color: Color) -> i32 {
        score
            * match color {
                Color::White => 1,
                Color::Black => -1,
            }
    }

    pub fn mate_in(ply: usize, color: Color) -> i32 {
        Self::score_color(Self::MAX - ply as i32, color)
    }
    
    // Distance to mate in UCI's unit: whole moves, signed. Positive when the side
    // to move delivers the mate, negative when it is the one being mated.
    pub fn mate_distance(score: i32) -> i32 {
        let plies = Self::MAX - score.abs();
        ((plies + 1) / 2) * score.signum()
    }

    pub fn is_mate(score: i32) -> bool {
        score.abs() > 40000
    }
}
