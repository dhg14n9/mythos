use crate::types::piece::PieceType;
use crate::types::Square;

// A move is u16, 4 bits for MoveKind, 6 bits for start square, 6 bits for destination square
// null move = 0
// 15         12 11         6 5           0
// +------------+------------+------------+
// |  kind (4)  |   to (6)   |  from (6)  |
// +------------+------------+------------+
#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash, Default)]
pub struct Move(u16);


// Move kind. Check out "https://www.chessprogramming.org/Encoding_Moves#From-To_Based"
#[derive(Copy, Clone)]
#[rustfmt::skip]
#[repr(u8)]
pub enum MoveKind {
    Normal = 0b0000,
    DoublePush = 0b0001,
    KingCastle = 0b0010,
    QueenCastle = 0b0011,
    Capture = 0b0100,
    EnPassant = 0b0101,

    PromoKnight = 0b1000,
    PromoBishop = 0b1001,
    PromoRook = 0b1010,
    PromoQueen = 0b1011,
    CapPromoKnight = 0b1100,
    CapPromoBishop = 0b1101,
    CapPromoRook = 0b1110,
    CapPromoQueen = 0b1111
}

impl Move {
    pub fn new(from: Square, to: Square, kind: MoveKind) -> Self {
        Move(((from as u16) & 0b0011_1111) | (((to as u16) & 0b0011_1111) << 6) | ((kind as u16) << 12))
    }

    pub fn from(self) -> Square {
        Square::new((self.0 & 0b0011_1111) as u8)
    }
    pub fn to(self) -> Square {
        Square::new(((self.0 >> 6) & 0b0011_1111) as u8)
    }
    pub fn kind(self) -> MoveKind {
        unsafe { std::mem::transmute((self.0 >> 12) as u8) }
    }

    pub fn is_null(self) -> bool {
        self.0 == 0
    }
    pub fn is_present(self) -> bool {
        self.0 != 0
    }

    // Moves that is a capture or a queen promotion
    pub fn is_noisy(self) -> bool {
        (self.kind() as u8 & 7) > MoveKind::KingCastle as u8
    }
    pub fn is_quiet(self) -> bool {
        self.is_present() && !self.is_noisy()
    }

    pub fn is_promotion(self) -> bool {
        (self.0 & 8) != 0
    }

    // special move are move that is neither Normal nor Capture.
    pub fn is_special(self) -> bool {
        (self.kind() as u8 & 11) != 0
    }

    pub fn is_enpassant(self) -> bool {
        self.kind() as u8 == 5
    }
    pub fn is_double_push(self) -> bool {
        self.kind() as u8 == 1
    }
    pub fn is_capture(self) -> bool {
        self.0 & 4 != 0
    }
    pub fn is_castling(self) -> bool {
        match self.kind() {
            MoveKind::KingCastle => true,
            MoveKind::QueenCastle => true,
            _ => false
        }
    }

    // evil bit manipulation
    pub fn capture_square(self) -> Square {
        self.to() ^ (self.is_enpassant() as u8 * 8)
    }

    pub fn promo_piece(self) -> PieceType {
        unsafe { std::mem::transmute(((self.0 as u8) & 0b0000_0011) + 1) } // knight = 1
    }


}