use crate::types::{Castling, Color, File, Piece, PieceType, Square};

type KeyType = u64;

const fn split_mix64(state: &mut KeyType) -> KeyType {
    *state = state.wrapping_add(0x9E3779B97F4A7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

struct ZobristKeys {
    square_key: [[KeyType; Piece::NUM]; Square::NUM],
    ep_file_key: [KeyType; File::NUM],
    castling_key: [KeyType; Castling::NUM],
    btm_key: KeyType // black to move key
}

const fn build_key() -> ZobristKeys {
    let mut state: KeyType = 0x9F4A7C15B97F4A7C;

    let mut square_key = [[0; Piece::NUM]; Square::NUM];
    let mut sq = 0;
    while sq < Square::NUM {
        let mut piece = 0;
        while piece < Piece::NUM {
            square_key[sq][piece] = split_mix64(&mut state);
            piece += 1;
        }
        sq += 1;
    }

    let mut ep_file_key = [0; File::NUM];
    let mut f = 0;
    while f < File::NUM {
        ep_file_key[f] = split_mix64(&mut state);
        f += 1;
    }

    let mut castling_key = [0; Castling::NUM];
    let mut c = 0;
    while c < Castling::NUM {
        castling_key[c] = split_mix64(&mut state);
        c += 1;
    }

    let btm_key = split_mix64(&mut state);

    ZobristKeys {
        square_key,
        ep_file_key,
        castling_key,
        btm_key,
    }
}

static zobrist_key: ZobristKeys = build_key();

#[derive(Copy, Clone)]
pub struct ZobristHelper;

impl ZobristHelper {

    pub fn toggle_square(zobrist: &mut u64, square: Square, piece: Piece) {
        *zobrist ^= zobrist_key.square_key[square][piece]
    }

    pub fn toggle_ep(zobrist: &mut u64, file: File) {
        *zobrist ^= zobrist_key.ep_file_key[file]
    }

    pub fn toggle_castling(zobrist: &mut u64, castling: Castling) {
        *zobrist ^= zobrist_key.castling_key[castling.raw()]
    }

    pub fn toggle_color(zobrist: &mut u64) {
        *zobrist ^= zobrist_key.btm_key
    }

}


