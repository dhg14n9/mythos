use crate::board::board::Board;
use crate::nnue::accumulator::king_context;
use crate::nnue::network::{evaluate, l1_int, pairwise_int, refresh};
use crate::nnue::{output_bucket, BUCKET_DIVISOR, HL, L1, NETWORK, OUTPUT_BUCKETS};
use crate::types::{Color, Piece, PieceType, Square};

pub fn print(board: &Board) {
    println!();
    for rank in (0..8u8).rev() {
        print!(" {} ", rank + 1);
        for file in 0..8u8 {
            print!(" {}", board.piece_at(Square::new(rank * 8 + file)));
        }
        println!();
    }
    println!("\n    a b c d e f g h\n");
    println!("fen:      {}", board.to_fen());
    println!("to move:  {}", board.stm());
    println!();

    for color in Color::ALL {
        let king = board.piece_bb(Piece::new(color, PieceType::King)).lsb();
        let (mirror, bucket) = king_context(board, color);
        println!("{color:<5}  king {king}  king bucket {bucket}  mirrored {mirror}");
    }
    println!();

    let stm = board.stm();
    let us = refresh(&NETWORK, board, stm);
    let them = refresh(&NETWORK, board, !stm);

    let mut input = [0u8; HL];
    pairwise_int(&us, &mut input[..HL / 2]);
    pairwise_int(&them, &mut input[HL / 2..]);
    let live = |half: &[u8]| half.iter().filter(|&&x| x != 0).count();
    println!("pairwise nonzero:  stm {}/{}  ntm {}/{}", live(&input[..HL / 2]), HL / 2, live(&input[HL / 2..]), HL / 2);
    println!();

    let pieces = board.occ().pop_count();
    let used = output_bucket(board);
    println!("  bucket  pieces  L1 active   eval (stm)");
    for bucket in 0..OUTPUT_BUCKETS {
        let low = bucket * BUCKET_DIVISOR + 2;
        let active = l1_int(&NETWORK, &input, bucket).iter().filter(|&&x| x > 0.0).count();
        let score = evaluate(&NETWORK, &us, &them, bucket);
        let marker = if bucket == used { ">" } else { " " };
        println!("{marker} {bucket:>5}   {low:>2}-{:<2}    {active:>2}/{L1}      {score:>+6}", low + BUCKET_DIVISOR - 1);
    }
    println!();

    let score = evaluate(&NETWORK, &us, &them, used);
    let white = if stm == Color::White { score } else { -score };
    println!("pieces {pieces}, output bucket {used}");
    println!("eval: {score:+} cp (side to move), {white:+} cp (white)");
}
