use crate::types::{Bitboard, File};

const fn gen_adjacent_file() -> [Bitboard; 8] {
    let mut result: [Bitboard; 8] = [Bitboard::EMPTY; 8];
    let mut i = 0;
    while i < 8 {
        let file_bb = Bitboard::from_file(File::ALL[i]).0;
        result[i] = Bitboard((file_bb & Bitboard::NOT_FILE_A.0) >> 1 | (file_bb & Bitboard::NOT_FILE_H.0) << 1);
        i += 1;
    }

    result
}

const fn gen_front_span() -> [[Bitboard; 64]; 2] {
    let mut result = [[Bitboard::EMPTY; 64]; 2];
    let mut i = 0;
    while i < 64 {
        let w = Bitboard(1 << i).north_fill();
        let w = Bitboard(w.0 ^ (1 << i));
        let b = Bitboard(1 << i).south_fill();
        let b = Bitboard(b.0 ^ (1 << i));

        result[0][i] = w;
        result[1][i] = b;

        i += 1;
    }

    result
}

const fn gen_passed_mask() -> [[Bitboard; 64]; 2] {
    let mut result = [[Bitboard::EMPTY; 64]; 2];
    let mut i = 0;
    while i < 64 {
        let w = Bitboard(1 << i).north_fill();
        let mut w = Bitboard(w.0 ^ (1 << i));
        let b = Bitboard(1 << i).south_fill();
        let mut b = Bitboard(b.0 ^ (1 << i));

        w.0 |= (w.0 & Bitboard::NOT_FILE_A.0) >> 1 | (w.0 & Bitboard::NOT_FILE_H.0) << 1;
        b.0 |= (b.0 & Bitboard::NOT_FILE_A.0) >> 1 | (b.0 & Bitboard::NOT_FILE_H.0) << 1;

        result[0][i] = w;
        result[1][i] = b;

        i += 1;
    }

    result
}

const ADJACENT_FILES: [Bitboard; 8] = gen_adjacent_file();
const FRONT_SPAN: [[Bitboard; 64]; 2] = gen_front_span();
const PASSED_MASK: [[Bitboard; 64]; 2] = gen_passed_mask();




#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Square;

    #[test]
    fn masks() {
        let sq = |s: &str| Square::parse(s).unwrap() as usize;

        // adjacent files: edge files touch 1 neighbour, middle files 2
        assert_eq!(ADJACENT_FILES[0].pop_count(), 8);  // a -> b
        assert_eq!(ADJACENT_FILES[3].pop_count(), 16); // d -> c,e

        // front span: squares strictly ahead on own file
        assert_eq!(FRONT_SPAN[0][sq("a2")].pop_count(), 6);
        assert_eq!(FRONT_SPAN[1][sq("a7")].pop_count(), 6);
        assert_eq!(FRONT_SPAN[0][sq("a8")].pop_count(), 0);
        assert_eq!(FRONT_SPAN[1][sq("a1")].pop_count(), 0);

        // passed mask: own + adjacent files, ranks strictly ahead
        assert_eq!(PASSED_MASK[0][sq("a2")].pop_count(), 12); // a,b x ranks 3-8
        assert_eq!(PASSED_MASK[1][sq("a7")].pop_count(), 12);
        assert_eq!(PASSED_MASK[0][sq("d4")].pop_count(), 12); // c,d,e x ranks 5-8
        assert_eq!(PASSED_MASK[0][sq("d8")].pop_count(), 0);
    }
}
