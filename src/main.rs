use mythos::types::Bitboard;

fn main() {
    let mut bb = Bitboard(u64::MAX);
    println!("{}", bb.0);
    bb <<= 1;
    println!("{}", bb.0);
}
