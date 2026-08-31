pub struct Score;

impl Score {
    pub const ZERO: i32 = 0;
    pub const MAX: i32 = 50000;
    pub const INF: i32 = 50001;
    pub const NONE: i32 = 50002;

    pub fn mate_in(ply: usize) -> i32 {
        Self::MAX - ply as i32
    }

    pub fn mated_in(ply: usize) -> i32 {
        -Self::MAX + ply as i32
    }

    pub fn to_tt(score: i32, ply: usize) -> i32 {
        if Self::is_mate(score) {
            score + score.signum() * ply as i32
        } else {
            score
        }
    }

    pub fn from_tt(score: i32, ply: usize) -> i32 {
        if Self::is_mate(score) {
            score - score.signum() * ply as i32
        } else {
            score
        }
    }

    pub fn mate_distance(score: i32) -> i32 {
        let plies = Self::MAX - score.abs();
        ((plies + 1) / 2) * score.signum()
    }

    pub fn is_mate(score: i32) -> bool {
        score.abs() > 40000
    }
}
