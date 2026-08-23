use crate::board::board::Board;
use crate::types::MoveList;
use crate::uci::gen_openings;

// Collect the stream the OpenBench client would have scraped.
fn generate(count: usize, seed: u64) -> Vec<String> {
    let mut fens = Vec::new();
    gen_openings(count, seed, None, |fen| fens.push(fen.to_string()));
    fens
}

// The client feeds these straight back to fastchess as opening positions, so a
// FEN that does not parse, or that is already over, poisons a whole workload.
fn check_playable(fens: &[String]) {
    for fen in fens {
        let board = Board::from_fen(fen).unwrap_or_else(|e| panic!("bad FEN {fen}: {e}"));
        assert_eq!(&board.to_fen(), fen, "FEN does not round trip: {fen}");

        let mut list = MoveList::new();
        board.gen_move(&mut list, false);
        assert!(list.len() > 0, "opening has no legal moves: {fen}");
    }
}

#[test]
fn openings_are_playable() {
    let fens = generate(100, 0xC0FFEE);
    assert_eq!(fens.len(), 100);
    check_playable(&fens);
}

// The client varies only the seed across threads and relies on that for
// variety, so the seed has to be the single source of randomness.
#[test]
fn same_seed_is_reproducible() {
    assert_eq!(generate(8, 1234), generate(8, 1234));
}

#[test]
fn different_seeds_differ() {
    assert_ne!(generate(8, 1), generate(8, 2));
}

// The volume a real workload asks for. Too slow for a debug build to run on
// every `cargo test`: reach it with `cargo test --release -- --ignored`.
#[test]
#[ignore]
fn bulk_openings_are_playable() {
    let fens = generate(2000, 0xDA7A6E4);
    assert_eq!(fens.len(), 2000);
    check_playable(&fens);
}
