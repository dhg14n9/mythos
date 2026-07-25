use crate::board::board::Board;
use crate::board::lookup::{bishop_attack, king_attack, knight_attack, queen_attack, rook_attack};
use crate::eval::{s_color, S};
use crate::types::{Color, Piece, PieceType};

const ATTACK_WEIGHT: [i32; 6] = [0, 2, 2, 3, 5, 0];

const DANGER_SCALE: i32 = 10;
const DANGER_CAP: i32 = 400;

pub fn king_safety(board: &Board) -> S {
    let mut result = S(0, 0);
    let occ = board.occ();

    for color in Color::ALL {
        let king_sq = board.piece_bb(Piece::new(color, PieceType::King)).lsb();
        let them = !color;
        let king_zone = king_attack(king_sq);

        let mut attacker = 0;
        let mut danger_level = 0;

        // queen attack
        for sq in board.piece_bb(Piece::new(them, PieceType::Queen)) {
            let attack = queen_attack(occ, sq);
            let hit = (attack & king_zone).pop_count();
            if hit > 0 {
                attacker += 1;
                danger_level += ATTACK_WEIGHT[4] * hit as i32;
            }
        }

        // rook attack
        for sq in board.piece_bb(Piece::new(them, PieceType::Rook)) {
            let attack = rook_attack(occ, sq);
            let hit = (attack & king_zone).pop_count();
            if hit > 0 {
                attacker += 1;
                danger_level += ATTACK_WEIGHT[3] * hit as i32;
            }
        }

        // bishop attack
        for sq in board.piece_bb(Piece::new(them, PieceType::Bishop)) {
            let attack = bishop_attack(occ, sq);
            let hit = (attack & king_zone).pop_count();
            if hit > 0 {
                attacker += 1;
                danger_level += ATTACK_WEIGHT[2] * hit as i32;
            }
        }

        // knight attack
        for sq in board.piece_bb(Piece::new(them, PieceType::Knight)) {
            let attack = knight_attack(sq);
            let hit = (attack & king_zone).pop_count();
            if hit > 0 {
                attacker += 1;
                danger_level += ATTACK_WEIGHT[1] * hit as i32;
            }
        }

        if attacker >= 2 {
            let penalty = (danger_level * danger_level / DANGER_SCALE).min(DANGER_CAP);
            result -= s_color(S(penalty, penalty / 4), color);
        }
    }

    result
}