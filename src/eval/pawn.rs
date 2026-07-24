use crate::board::board::Board;
use crate::eval::{s_color, S};
use crate::types::{Bitboard, Color, Piece};

const PASS_PAWN_BONUS: [S; 8] = [
    S(0,0), S(0,5), S(0,10), S(5,25), S(20,50), S(40,90), S(65,140), S(0,0)
];

const ISO_PAWN_MALUS: S = S(5, 15);
const DOUBLE_PAWN_MALUS: S = S(10, 20);

fn spread(bb: Bitboard) -> Bitboard {
    ((bb & Bitboard::NOT_FILE_A) >> 1) | ((bb & Bitboard::NOT_FILE_H) << 1)
}

pub fn pawns(board: &Board) -> S {
    pawn_structure(board.piece_bb(Piece::WhitePawn), board.piece_bb(Piece::BlackPawn))
}

fn pawn_structure(white: Bitboard, black: Bitboard) -> S {
    let mut result = S(0, 0);
    let pawn_bb = [white, black];

    for color in Color::ALL {
        let own = pawn_bb[color];
        let opp = pawn_bb[!color];

        let own_north = own.north_fill();
        let own_south = own.south_fill();

        let opp_wide = opp | spread(opp);

        let (own_ahead, opp_ahead) = match color {
            Color::White => (own_south >> 8, opp_wide.south_fill() >> 8),
            Color::Black => (own_north << 8, opp_wide.north_fill() << 8),
        };

        for sq in own & !own_ahead & !opp_ahead {
            result += s_color(PASS_PAWN_BONUS[sq.relative_to(color).rank()], color);
        }

        let iso = (own & !spread(own_north | own_south)).pop_count() as i32;
        result -= s_color(ISO_PAWN_MALUS * iso, color);

        let doubled = (own & own_ahead).pop_count() as i32;
        result -= s_color(DOUBLE_PAWN_MALUS * doubled, color);
    }

    result
}
