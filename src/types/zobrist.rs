use crate::types::{Castling, File, Piece, Square};

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

static ZOBRIST_KEY: ZobristKeys = build_key();

#[derive(Copy, Clone)]
pub struct ZobristHelper;

impl ZobristHelper {

    pub fn square(square: Square, piece: Piece) -> u64 {
        ZOBRIST_KEY.square_key[square][piece]
    }

    pub fn ep(file: File) -> u64 {
        ZOBRIST_KEY.ep_file_key[file]
    }

    pub fn castling(castling: Castling) -> u64 {
        ZOBRIST_KEY.castling_key[castling.raw()]
    }

    pub fn color() -> u64 {
        ZOBRIST_KEY.btm_key
    }

}


