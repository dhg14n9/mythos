const fn bit(rank: i32, file: i32) -> u64 {
    1u64 << (rank * 8 + file) as u32
}

const fn rook_mask(sq: usize) -> u64 {
    let r = (sq / 8) as i32;
    let f = (sq % 8) as i32;
    let mut mask = 0u64;

    let mut rr = r + 1; while rr <= 6 { mask |= bit(rr, f); rr += 1; }
    let mut rr = r - 1; while rr >= 1 { mask |= bit(rr, f); rr -= 1; }
    let mut ff = f + 1; while ff <= 6 { mask |= bit(r, ff); ff += 1; }
    let mut ff = f - 1; while ff >= 1 { mask |= bit(r, ff); ff -= 1; }

    mask
}

const fn bishop_mask(sq: usize) -> u64 {
    let r = (sq / 8) as i32;
    let f = (sq % 8) as i32;
    let mut mask = 0u64;

    let (mut rr, mut ff) = (r + 1, f + 1); while rr <= 6 && ff <= 6 { mask |= bit(rr, ff); rr += 1; ff += 1; }
    let (mut rr, mut ff) = (r + 1, f - 1); while rr <= 6 && ff >= 1 { mask |= bit(rr, ff); rr += 1; ff -= 1; }
    let (mut rr, mut ff) = (r - 1, f + 1); while rr >= 1 && ff <= 6 { mask |= bit(rr, ff); rr -= 1; ff += 1; }
    let (mut rr, mut ff) = (r - 1, f - 1); while rr >= 1 && ff >= 1 { mask |= bit(rr, ff); rr -= 1; ff -= 1; }

    mask
}

const fn gen_rook_attack(sq: usize, blocker: u64) -> u64 {
    let r = (sq / 8) as i32;
    let f = (sq % 8) as i32;
    let mut att = 0u64;

    let mut ff = f - 1; while ff >= 0 { let b = bit(r, ff); att |= b; if blocker & b != 0 { break; } ff -= 1; }
    let mut ff = f + 1; while ff <= 7 { let b = bit(r, ff); att |= b; if blocker & b != 0 { break; } ff += 1; }
    let mut rr = r + 1; while rr <= 7 { let b = bit(rr, f); att |= b; if blocker & b != 0 { break; } rr += 1; }
    let mut rr = r - 1; while rr >= 0 { let b = bit(rr, f); att |= b; if blocker & b != 0 { break; } rr -= 1; }

    att
}


const fn gen_bishop_attack(sq: usize, blocker: u64) -> u64 {
    let r = (sq / 8) as i32;
    let f = (sq % 8) as i32;
    let mut att = 0u64;

    let (mut rr, mut ff) = (r + 1, f + 1); while rr <= 7 && ff <= 7 { let b = bit(rr, ff); att |= b; if blocker & b != 0 { break; } rr += 1; ff += 1; }
    let (mut rr, mut ff) = (r + 1, f - 1); while rr <= 7 && ff >= 0 { let b = bit(rr, ff); att |= b; if blocker & b != 0 { break; } rr += 1; ff -= 1; }
    let (mut rr, mut ff) = (r - 1, f + 1); while rr >= 0 && ff <= 7 { let b = bit(rr, ff); att |= b; if blocker & b != 0 { break; } rr -= 1; ff += 1; }
    let (mut rr, mut ff) = (r - 1, f - 1); while rr >= 0 && ff >= 0 { let b = bit(rr, ff); att |= b; if blocker & b != 0 { break; } rr -= 1; ff -= 1; }

    att
}


const fn nth_subset(mask: u64, n: usize) -> u64 {
    let mut result = 0u64;
    let mut m = mask;
    let mut i = 0;
    while m != 0 {
        let low = m & m.wrapping_neg();
        if (n >> i) & 1 != 0 {
            result |= low;
        }
        m &= m - 1;
        i += 1;
    }
    result
}

const fn table_size() -> usize {
    let mut total = 0usize;
    let mut sq = 0;
    while sq < 64 {
        total += 1usize << rook_mask(sq).count_ones();
        total += 1usize << bishop_mask(sq).count_ones();
        sq += 1;
    }
    total
}


// Magic numbers
#[cfg(not(all(target_arch = "x86_64", target_feature = "bmi2")))]
pub const ROOK_MAGICS: [(u64, u32); 64] = [
    (0x2080001040002081, 52),  // a1
    (0x0440041000200040, 53),  // b1
    (0x0080100080200008, 53),  // c1
    (0x2080045000808800, 53),  // d1
    (0x9200080560101600, 53),  // e1
    (0x0200080104104200, 53),  // f1
    (0x020004080100c200, 53),  // g1
    (0x4100030001288052, 52),  // h1
    (0x1012800080204000, 53),  // a2
    (0x2c80400040201000, 54),  // b2
    (0x0002001282420222, 54),  // c2
    (0x8904801004805800, 54),  // d2
    (0x0104800400480080, 54),  // e2
    (0x3002002409020050, 54),  // f2
    (0x4004008830020401, 54),  // g2
    (0x000a000400894205, 53),  // h2
    (0x0480084020004000, 53),  // a3
    (0x0020004000500020, 54),  // b3
    (0x8000110043002000, 54),  // c3
    (0x8844808008001001, 54),  // d3
    (0x0050808004000802, 54),  // e3
    (0x8000808002000400, 54),  // f3
    (0x0013040002900148, 54),  // g3
    (0x14040a0000591084, 53),  // h3
    (0x0000800080204000, 53),  // a4
    (0x0008400a80200482, 54),  // b4
    (0x0008248200120041, 54),  // c4
    (0x44450421000a1000, 54),  // d4
    (0x00b1100500080100, 54),  // e4
    (0x0001040080020080, 54),  // f4
    (0x1000820400015008, 54),  // g4
    (0x8261040600008071, 53),  // h4
    (0xc040400080800026, 53),  // a5
    (0x0040200880804006, 54),  // b5
    (0x0000806002801001, 54),  // c5
    (0x0001040821001000, 54),  // d5
    (0x2020050011000800, 54),  // e5
    (0x06a4000200808004, 54),  // f5
    (0x1244011004000248, 54),  // g5
    (0x200100004100129a, 53),  // h5
    (0x0000400020808001, 53),  // a6
    (0x0000408211020020, 54),  // b6
    (0x2000806202d20040, 54),  // c6
    (0x8400090010010020, 54),  // d6
    (0x0006050008010010, 54),  // e6
    (0x0002010408020010, 54),  // f6
    (0x0002000104020008, 54),  // g6
    (0x0280141044820005, 53),  // h6
    (0x0001020020408200, 53),  // a7
    (0x3040068246210b00, 54),  // b7
    (0x0000200993004100, 54),  // c7
    (0x0010008008001180, 54),  // d7
    (0x0212c50008005100, 54),  // e7
    (0x0110040002008080, 54),  // f7
    (0x0004800200010080, 54),  // g7
    (0x08040040810c0200, 53),  // h7
    (0x0080008410204501, 52),  // a8
    (0x1003008240001423, 53),  // b8
    (0x01108c40d1006001, 53),  // c8
    (0x024200081004a142, 53),  // d8
    (0x2002002104100802, 53),  // e8
    (0x0082009001080402, 53),  // f8
    (0x0205000402000081, 53),  // g8
    (0x1000002104008042, 52),  // h8
];

#[cfg(not(all(target_arch = "x86_64", target_feature = "bmi2")))]
pub const BISHOP_MAGICS: [(u64, u32); 64] = [
    (0x004008282089a040, 58),  // a1
    (0x1050149802484000, 59),  // b1
    (0x011008820040c278, 59),  // c1
    (0x0088094500720210, 59),  // d1
    (0x4008484080026000, 59),  // e1
    (0x0002032420400000, 59),  // f1
    (0x20ca084108082c00, 59),  // g1
    (0x10406101101002aa, 58),  // h1
    (0x4600102012048208, 59),  // a2
    (0x11802802080a0030, 59),  // b2
    (0x0012101880810400, 59),  // c2
    (0x0000140420800208, 59),  // d2
    (0x1208045040040910, 59),  // e2
    (0x801002015008c008, 59),  // f2
    (0x5000008088205040, 59),  // g2
    (0x0000810101112020, 59),  // h2
    (0x04980164102c0810, 59),  // a3
    (0x0004211204082210, 59),  // b3
    (0x2004004848002104, 57),  // c3
    (0x800800008200c040, 57),  // d3
    (0x0024001280a00600, 57),  // e3
    (0x1a01080201092000, 57),  // f3
    (0x0016000442262014, 59),  // g3
    (0x0001012681809010, 59),  // h3
    (0x4008041019105020, 59),  // a4
    (0x8010080130014302, 59),  // b4
    (0x1102080040428220, 57),  // c4
    (0x4020802108020020, 55),  // d4
    (0x4008840020802000, 55),  // e4
    (0x1030004006880814, 57),  // f4
    (0x4001040109248800, 59),  // g4
    (0x2006009002208824, 59),  // h4
    (0x48080240009818c8, 59),  // a5
    (0x2604042400a021a0, 59),  // b5
    (0x1000841000150540, 57),  // c5
    (0x20a0404800228200, 55),  // d5
    (0x08081008200c0020, 55),  // e5
    (0x18d1010200010800, 57),  // f5
    (0x0008480100046500, 59),  // g5
    (0x2004240120004110, 59),  // h5
    (0x0092286008000410, 59),  // a6
    (0x1062c40420088c08, 59),  // b6
    (0x9002005404180a00, 57),  // c6
    (0x0101004010400200, 57),  // d6
    (0x0000c03009040180, 57),  // e6
    (0x2aa0018105400600, 57),  // f6
    (0x0004080283040405, 59),  // g6
    (0x200404005204c045, 59),  // h6
    (0x0004040208040c00, 59),  // a7
    (0x0414208818084801, 59),  // b7
    (0x0014004200901000, 59),  // c7
    (0x4522000020882000, 59),  // d7
    (0x01420160042c0c04, 59),  // e7
    (0x0010902101410102, 59),  // f7
    (0x0240700201852404, 59),  // g7
    (0x04200402c6004000, 59),  // h7
    (0x4030440288280200, 58),  // a8
    (0x0802010901012082, 59),  // b8
    (0x0008100104020b00, 59),  // c8
    (0x0004210160940401, 59),  // d8
    (0x0812010410420a1b, 59),  // e8
    (0x0802020404480200, 59),  // f8
    (0x4200252008060f84, 59),  // g8
    (0x880421020c240880, 58),  // h8
];


const TABLE_SIZE: usize = table_size();


#[cfg(not(all(target_arch = "x86_64", target_feature = "bmi2")))]
mod imp {
    use super::*;
    use crate::types::{Bitboard, Square};

    #[derive(Copy, Clone)]
    struct Magic {
        mask: u64,
        magic: u64,
        shift: u8,
        start: u32,
    }

    struct MagicTables {
        rook: [Magic; Square::NUM],
        bishop: [Magic; Square::NUM],
        table: [Bitboard; TABLE_SIZE],
    }

    const fn build() -> MagicTables {
        let mut rook = [Magic { mask: 0, magic: 0, shift: 0, start: 0 }; Square::NUM];
        let mut bishop = [Magic { mask: 0, magic: 0, shift: 0, start: 0 }; Square::NUM];
        let mut table = [Bitboard::EMPTY; TABLE_SIZE];
        let mut current = 0usize;

        let mut sq = 0;
        while sq < 64 {
            let mask = rook_mask(sq);
            let (magic, shift) = ROOK_MAGICS[sq];
            let size = 1usize << mask.count_ones();
            rook[sq] = Magic { mask, magic, shift: shift as u8, start: current as u32 };

            let mut i = 0;
            while i < size {
                let blocker = nth_subset(mask, i);
                let idx = current + (blocker.wrapping_mul(magic) >> shift) as usize;
                let attack = gen_rook_attack(sq, blocker);
                // No legal slider attack is empty, so a non-empty slot with a
                // different value can only be a magic collision -> fail the build.
                if table[idx].0 != 0 && table[idx].0 != attack {
                    panic!("rook magic collision");
                }
                table[idx] = Bitboard(attack);
                i += 1;
            }

            current += size;
            sq += 1;
        }

        let mut sq = 0;
        while sq < 64 {
            let mask = bishop_mask(sq);
            let (magic, shift) = BISHOP_MAGICS[sq];
            let size = 1usize << mask.count_ones();
            bishop[sq] = Magic { mask, magic, shift: shift as u8, start: current as u32 };

            let mut i = 0;
            while i < size {
                let blocker = nth_subset(mask, i);
                let idx = current + (blocker.wrapping_mul(magic) >> shift) as usize;
                let attack = gen_bishop_attack(sq, blocker);
                if table[idx].0 != 0 && table[idx].0 != attack {
                    panic!("bishop magic collision");
                }
                table[idx] = Bitboard(attack);
                i += 1;
            }

            current += size;
            sq += 1;
        }

        MagicTables { rook, bishop, table }
    }

    static TABLES: MagicTables = build();

    pub fn rook_attack(occ: Bitboard, square: Square) -> Bitboard {
        let m = &TABLES.rook[square as usize];
        let idx = (occ.0 & m.mask).wrapping_mul(m.magic) >> m.shift;
        TABLES.table[m.start as usize + idx as usize]
    }

    pub fn bishop_attack(occ: Bitboard, square: Square) -> Bitboard {
        let m = &TABLES.bishop[square as usize];
        let idx = (occ.0 & m.mask).wrapping_mul(m.magic) >> m.shift;
        TABLES.table[m.start as usize + idx as usize]
    }

    pub fn queen_attack(occ: Bitboard, square: Square) -> Bitboard {
        bishop_attack(occ, square) | rook_attack(occ, square)
    }
}

#[cfg(all(target_arch = "x86_64", target_feature = "bmi2"))]
mod imp {
    use super::*;
    use crate::types::{Bitboard, Square};
    use std::arch::x86_64::_pext_u64;

    #[derive(Copy, Clone)]
    struct Pext {
        mask: u64,
        start: u32,
    }

    struct PextTables {
        rook: [Pext; Square::NUM],
        bishop: [Pext; Square::NUM],
        table: [Bitboard; TABLE_SIZE],
    }

    const fn build() -> PextTables {
        let mut rook = [Pext { mask: 0, start: 0 }; Square::NUM];
        let mut bishop = [Pext { mask: 0, start: 0 }; Square::NUM];
        let mut table = [Bitboard::EMPTY; TABLE_SIZE];
        let mut current = 0usize;

        let mut sq = 0;
        while sq < 64 {
            let mask = rook_mask(sq);
            let size = 1usize << mask.count_ones();
            rook[sq] = Pext { mask, start: current as u32 };

            // pext(nth_subset(mask, i), mask) == i, so the slot is just `i`.
            // That keeps the builder intrinsic-free and therefore `const`.
            let mut i = 0;
            while i < size {
                table[current + i] = Bitboard(gen_rook_attack(sq, nth_subset(mask, i)));
                i += 1;
            }

            current += size;
            sq += 1;
        }

        let mut sq = 0;
        while sq < 64 {
            let mask = bishop_mask(sq);
            let size = 1usize << mask.count_ones();
            bishop[sq] = Pext { mask, start: current as u32 };

            let mut i = 0;
            while i < size {
                table[current + i] = Bitboard(gen_bishop_attack(sq, nth_subset(mask, i)));
                i += 1;
            }

            current += size;
            sq += 1;
        }

        PextTables { rook, bishop, table }
    }

    static TABLES: PextTables = build();

    pub fn rook_attack(occ: Bitboard, square: Square) -> Bitboard {
        let e = &TABLES.rook[square as usize];
        // `_pext_u64` already ignores bits outside the mask, so no pre-AND needed.
        let idx = unsafe { _pext_u64(occ.0, e.mask) } as usize;
        TABLES.table[e.start as usize + idx]
    }

    pub fn bishop_attack(occ: Bitboard, square: Square) -> Bitboard {
        let e = &TABLES.bishop[square as usize];
        let idx = unsafe { _pext_u64(occ.0, e.mask) } as usize;
        TABLES.table[e.start as usize + idx]
    }

    pub fn queen_attack(occ: Bitboard, square: Square) -> Bitboard {
        bishop_attack(occ, square) | rook_attack(occ, square)
    }
}

pub use imp::{rook_attack, bishop_attack, queen_attack};


mod leapers {
    use super::bit;
    use crate::types::{Bitboard, Color, Square};

    const fn leaper_table(offsets: &[(i32, i32)]) -> [Bitboard; 64] {
        let mut table = [Bitboard::EMPTY; 64];
        let mut sq = 0;
        while sq < 64 {
            let r = (sq / 8) as i32;
            let f = (sq % 8) as i32;
            let mut att = 0u64;

            let mut i = 0;
            while i < offsets.len() {
                let (dr, df) = offsets[i];
                let rr = r + dr;
                let ff = f + df;
                if rr >= 0 && rr <= 7 && ff >= 0 && ff <= 7 {
                    att |= bit(rr, ff);
                }
                i += 1;
            }

            table[sq] = Bitboard(att);
            sq += 1;
        }
        table
    }


    const fn pawn_table() -> [[Bitboard; 64]; Color::NUM] {
        let mut table = [[Bitboard::EMPTY; 64]; Color::NUM];
        let mut sq = 0;
        while sq < 64 {
            let r = (sq / 8) as i32;
            let f = (sq % 8) as i32;

            let mut white = 0u64;
            if r + 1 <= 7 {
                if f - 1 >= 0 { white |= bit(r + 1, f - 1); }
                if f + 1 <= 7 { white |= bit(r + 1, f + 1); }
            }
            table[Color::White as usize][sq] = Bitboard(white);

            let mut black = 0u64;
            if r - 1 >= 0 {
                if f - 1 >= 0 { black |= bit(r - 1, f - 1); }
                if f + 1 <= 7 { black |= bit(r - 1, f + 1); }
            }
            table[Color::Black as usize][sq] = Bitboard(black);

            sq += 1;
        }
        table
    }

    static KNIGHT_ATTACKS: [Bitboard; 64] =
        leaper_table(&[(2, 1), (2, -1), (-2, 1), (-2, -1), (1, 2), (1, -2), (-1, 2), (-1, -2)]);
    static KING_ATTACKS: [Bitboard; 64] =
        leaper_table(&[(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)]);
    static PAWN_ATTACKS: [[Bitboard; 64]; Color::NUM] = pawn_table();

    pub fn knight_attack(square: Square) -> Bitboard {
        KNIGHT_ATTACKS[square as usize]
    }

    pub fn king_attack(square: Square) -> Bitboard {
        KING_ATTACKS[square as usize]
    }

    pub fn pawn_attack(color: Color, square: Square) -> Bitboard {
        PAWN_ATTACKS[color as usize][square as usize]
    }
}

pub use leapers::{king_attack, knight_attack, pawn_attack};
