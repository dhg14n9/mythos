use crate::board::lookup::{bishop_attack, king_attack, knight_attack, pawn_attack, queen_attack, rook_attack};
use crate::types::{Bitboard, Color, Direction, Move, MoveKind, MoveList, Piece, PieceType, Rank, BETWEEN, RAY};
use super::board::Board;

impl Board {
    // us: color of the king that is being checked
    pub fn checkers(&self, us: Color) -> Bitboard {
        let them_bb = self.color_bb(!us);
        let king_square = self.piece_bb(Piece::new(us, PieceType::King)).lsb();
        let occ = self.occ();

        let queens = self.piece_type_bb(PieceType::Queen);
        let attackers =
            (bishop_attack(occ, king_square) & (self.piece_type_bb(PieceType::Bishop) | queens))
            | (rook_attack(occ, king_square) & (self.piece_type_bb(PieceType::Rook) | queens))
            | (knight_attack(king_square) & self.piece_type_bb(PieceType::Knight))
            | (pawn_attack(us, king_square) & self.piece_type_bb(PieceType::Pawn));

        attackers & them_bb
    }

    // us: color of the king being pinned to
    pub fn pinners(&self, us: Color) -> Bitboard {
        let us_bb = self.color_bb(us);
        let them_bb = self.color_bb(!us);
        let occ = self.occ();
        let king_square = self.piece_bb(Piece::new(us, PieceType::King)).lsb();

        let queens = self.piece_type_bb(PieceType::Queen);
        let snipers = ((rook_attack(Bitboard::EMPTY, king_square)
                & (self.piece_type_bb(PieceType::Rook) | queens))
            | (bishop_attack(Bitboard::EMPTY, king_square)
                & (self.piece_type_bb(PieceType::Bishop) | queens)))
            & them_bb;

        let mut result = Bitboard::EMPTY;
        for sq in snipers {
            let blockers = BETWEEN[sq][king_square] & occ;
            if blockers.pop_count() == 1 && !(blockers & us_bb).is_empty() {
                result.set(sq);
            }
        }

        result
    }

    // us: color of the king
    pub fn threats(&self, us: Color) -> Bitboard {
        let them = !us;
        let them_bb = self.color_bb(them);
        let queens = self.piece_type_bb(PieceType::Queen);

        let occ = self.occ() ^ self.piece_bb(Piece::new(us, PieceType::King));

        let mut threats = Bitboard::EMPTY;

        let pawns = self.piece_bb(Piece::new(them, PieceType::Pawn));
        let (left_dir, right_dir) = match them {
            Color::White => (Direction::UpLeft, Direction::UpRight),
            Color::Black => (Direction::DownLeft, Direction::DownRight),
        };
        let mut left = pawns;
        left.shift(left_dir);
        let mut right = pawns;
        right.shift(right_dir);
        threats |= left | right;

        for sq in self.piece_type_bb(PieceType::Knight) & them_bb {
            threats |= knight_attack(sq);
        }
        for sq in (self.piece_type_bb(PieceType::Bishop) | queens) & them_bb {
            threats |= bishop_attack(occ, sq);
        }
        for sq in (self.piece_type_bb(PieceType::Rook) | queens) & them_bb {
            threats |= rook_attack(occ, sq);
        }
        threats |= king_attack(self.piece_bb(Piece::new(them, PieceType::King)).lsb());

        threats
    }


    pub fn gen_move(&self, quiet_list: &mut MoveList, noisy_list: &mut MoveList) {
        quiet_list.clear();
        noisy_list.clear();
        let us = self.side_to_move;
        let them = !us;
        let threats = self.threats(us);
        let pinner = self.pinners(us);
        let checker = self.checkers(us);

        let us_bb = self.color_bb(us);
        let them_bb = self.color_bb(them);
        let occ = self.occ();
        let king_square = self.piece_bb(Piece::new(us, PieceType::King)).lsb();

        // king moves
        let king_target = king_attack(king_square) & !us_bb & !threats;
        let king_capture_target = king_target & them_bb;
        for to in king_capture_target {
            noisy_list.push(Move::new(king_square, to, MoveKind::Capture));
        }
        for to in king_target & !king_capture_target {
            quiet_list.push(Move::new(king_square, to, MoveKind::Normal))
        }

        // if its double check, only king moves are available
        let check_mask = match checker.pop_count() {
            0 => { Bitboard::FULL }
            1 => { let checker_square = checker.lsb(); BETWEEN[king_square][checker_square] | Bitboard::from_square(checker_square) }
            _ => { return; }
        };

        let mut pinned = Bitboard::EMPTY;
        for sniper in pinner {
            pinned |= BETWEEN[sniper][king_square] & us_bb
        }

        // rook
        for from in self.piece_bb(Piece::new(us, PieceType::Rook)) {
            let mut restriction = check_mask;
            if pinned.contains(from) { restriction &= RAY[king_square][from] }
            let target = rook_attack(occ, from) & !us_bb & restriction;

            let capture_target = target & them_bb;
            for to in capture_target {
                noisy_list.push(Move::new(from, to, MoveKind::Capture));
            }
            for to in target & !capture_target {
                quiet_list.push(Move::new(from, to, MoveKind::Normal));
            }
        }

        // bishop
        for from in self.piece_bb(Piece::new(us, PieceType::Bishop)) {
            let mut restriction = check_mask;
            if pinned.contains(from) { restriction &= RAY[king_square][from] }
            let target = bishop_attack(occ, from) & !us_bb & restriction;

            let capture_target = target & them_bb;
            for to in capture_target {
                noisy_list.push(Move::new(from, to, MoveKind::Capture));
            }
            for to in target & !capture_target {
                quiet_list.push(Move::new(from, to, MoveKind::Normal));
            }
        }

        // queen
        for from in self.piece_bb(Piece::new(us, PieceType::Queen)) {
            let mut restriction = check_mask;
            if pinned.contains(from) { restriction &= RAY[king_square][from] }
            let target = queen_attack(occ, from) & !us_bb & restriction;

            let capture_target = target & them_bb;
            for to in capture_target {
                noisy_list.push(Move::new(from, to, MoveKind::Capture));
            }
            for to in target & !capture_target {
                quiet_list.push(Move::new(from, to, MoveKind::Normal));
            }
        }

        // knight
        for from in self.piece_bb(Piece::new(us, PieceType::Knight)) {
            let mut restriction = check_mask;
            if pinned.contains(from) { restriction &= RAY[king_square][from] }
            let target = knight_attack(from) & !us_bb & restriction;

            let capture_target = target & them_bb;
            for to in capture_target {
                noisy_list.push(Move::new(from, to, MoveKind::Capture));
            }
            for to in target & !capture_target {
                quiet_list.push(Move::new(from, to, MoveKind::Normal));
            }
        }

        // pawn
        let forward: i8 = if us == Color::White { 8 } else { -8 };
        let promo_rank = Rank::PRE_PROMOTION_RANK[us];
        let start_rank = Rank::PAWN_START_RANK[us];

        for from in self.piece_bb(Piece::new(us, PieceType::Pawn)) {
            let mut restriction = check_mask;
            if pinned.contains(from) { restriction &= RAY[king_square][from] }

            // capture (no enpassant)
            let target = pawn_attack(us, from) & them_bb & restriction;
            for to in target {
                if from.rank() == promo_rank {
                    noisy_list.push(Move::new(from, to, MoveKind::CapPromoBishop));
                    noisy_list.push(Move::new(from, to, MoveKind::CapPromoRook));
                    noisy_list.push(Move::new(from, to, MoveKind::CapPromoKnight));
                    noisy_list.push(Move::new(from, to, MoveKind::CapPromoQueen));
                } else {
                    noisy_list.push(Move::new(from, to, MoveKind::Capture))
                }
            }

            // en passant
            if !self.en_passant.is_none() {
                let ep = self.en_passant;
                if pawn_attack(us, from).contains(ep) {
                    let cap = ep.offset(-forward); // the enemy pawn being removed
                    let on_pin_ray = !pinned.contains(from) || RAY[king_square][from].contains(ep);
                    let resolves_check = check_mask.contains(ep) || check_mask.contains(cap);
                    if on_pin_ray && resolves_check {
                        // because a snipper can pin a king through 2 pawns in an enpassant so this
                        // has to be checked separately
                        let occ_after =
                            occ ^ Bitboard::from_square(from) ^ Bitboard::from_square(cap);
                        let king_vision = rook_attack(occ_after, king_square)
                            & Bitboard::from_rank(king_square.rank());
                        if (king_vision
                            & (self.piece_bb(Piece::new(them, PieceType::Queen))
                                | self.piece_bb(Piece::new(them, PieceType::Rook)))
                        ).is_empty() {
                            noisy_list.push(Move::new(from, ep, MoveKind::EnPassant));
                        }
                    }
                }
            }


            // pawn push
            let to = from.offset(forward);
            if !occ.contains(to) & restriction.contains(to) {
                if from.rank() == promo_rank {
                    quiet_list.push(Move::new(from, to, MoveKind::PromoBishop));
                    quiet_list.push(Move::new(from, to, MoveKind::PromoRook));
                    quiet_list.push(Move::new(from, to, MoveKind::PromoKnight));
                    noisy_list.push(Move::new(from, to, MoveKind::PromoQueen));
                } else {
                    quiet_list.push(Move::new(from, to, MoveKind::Normal));
                }

                if from.rank() == start_rank {
                    let to2 = to.offset(forward);
                    if !occ.contains(to2) & restriction.contains(to2) {
                        quiet_list.push(Move::new(from, to2, MoveKind::DoublePush))
                    }
                }
            }
        }


    }
}

