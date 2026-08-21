use std::path::Path;

use crate::board::board::Board;
use crate::nnue::accumulator::feature_index;
use crate::nnue::network::{evaluate, load_net, refresh};
use crate::types::{Color, Piece, PieceType, Square};

const NET: &str = "nets/random.nnue";

const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

fn white(pt: PieceType) -> Piece {
    Piece::new(Color::White, pt)
}

fn black(pt: PieceType) -> Piece {
    Piece::new(Color::Black, pt)
}

// ---------------------------------------------------------------- feature_index

// The four hand-computed cases. Between them they pin every term of the
// formula: the 64 * piece_type stride, the "us"/"them" 384 offset, and the
// vertical flip applied for a black perspective.
#[test]
fn feature_index_hand_computed() {
    // "us", no mirror: 0 * 64 + 8 + 0
    assert_eq!(feature_index(Color::White, white(PieceType::Pawn), Square::A2), 8);

    // "them" AND mirrored: 0 * 64 + (8 ^ 56) + 384
    assert_eq!(feature_index(Color::Black, white(PieceType::Pawn), Square::A2), 432);

    // "them", no mirror: 0 * 64 + 48 + 384
    assert_eq!(feature_index(Color::White, black(PieceType::Pawn), Square::A7), 432);

    // pins the piece_type stride at King = 5: 5 * 64 + 4 + 0
    assert_eq!(feature_index(Color::White, white(PieceType::King), Square::E1), 324);
}

// "A white piece on square s, seen by Black" and "a black piece of the same
// type on the mirrored square, seen by White" are the SAME situation, so they
// must map to the same input. This holds only if both flips -- the colour
// offset and the rank mirror -- are right; breaking either one breaks it.
#[test]
fn feature_index_is_colour_mirror_symmetric() {
    for pt in PieceType::ALL {
        for sq in 0..64u8 {
            let square = Square::new(sq);
            let mirrored = square.flip_rank();

            assert_eq!(
                feature_index(Color::Black, white(pt), square),
                feature_index(Color::White, black(pt), mirrored),
                "{pt} on {square} broke mirror symmetry",
            );
        }
    }
}

// Every (perspective, piece, square) must land inside the 768 inputs, and no
// two distinct (piece, square) pairs may collide from one perspective -- a
// collision would silently merge two features into one.
#[test]
fn feature_index_is_in_range_and_injective() {
    for perspective in Color::ALL {
        let mut seen = [false; 768];

        for colour in Color::ALL {
            for pt in PieceType::ALL {
                for sq in 0..64u8 {
                    let idx = feature_index(perspective, Piece::new(colour, pt), Square::new(sq));

                    assert!(idx < 768, "index {idx} out of range");
                    assert!(!seen[idx], "index {idx} collided");
                    seen[idx] = true;
                }
            }
        }

        // 2 colours * 6 types * 64 squares == 768, so every input is claimed.
        assert!(seen.iter().all(|&s| s), "some inputs were never produced");
    }
}

// ---------------------------------------------------------------- forward pass

// The net is gitignored, so skip rather than fail on a fresh clone.
fn net_available() -> bool {
    if Path::new(NET).exists() {
        return true;
    }
    eprintln!("skipping: {NET} not present (generate it first)");
    false
}

// Mostly a "does a number come out" test -- with random weights the value is
// meaningless. What it really proves is that nothing overflows: `cargo test`
// builds in debug, where i16 overflow in `refresh` panics rather than wrapping.
#[test]
fn forward_pass_produces_a_score() {
    if !net_available() {
        return;
    }

    let net = load_net(NET);
    let board = Board::from_fen(STARTPOS).expect("bad FEN");

    let us = refresh(&net, &board, board.stm());
    let them = refresh(&net, &board, !board.stm());

    let score = evaluate(&net, &us, &them);
    println!("startpos raw nnue output: {score}");

    // Sanity bound only. A score outside this means the quantisation arithmetic
    // is wrong, not that the (random) net has an opinion.
    assert!(score.abs() < 100_000, "implausible score {score}");
}

// The strongest test here. These two positions are the same position mirrored:
// ranks flipped and colours swapped, so the side to move sees an identical
// board. The network's inputs must therefore be identical, and the evals equal.
//
//   A: white pawn e2, white king e1, black king h1, white to move
//   B: black pawn e7, black king e8, white king h8, black to move
//
// This exercises feature_index, both perspectives, and refresh end to end. A
// bug in either flip breaks it, while a symmetric position like startpos would
// not notice.
#[test]
fn mirrored_positions_evaluate_identically() {
    if !net_available() {
        return;
    }

    let net = load_net(NET);

    let a = Board::from_fen("8/8/8/8/8/8/4P3/4K2k w - - 0 1").expect("bad FEN");
    let b = Board::from_fen("4k2K/4p3/8/8/8/8/8/8 b - - 0 1").expect("bad FEN");

    let score_a = evaluate(
        &net,
        &refresh(&net, &a, a.stm()),
        &refresh(&net, &a, !a.stm()),
    );
    let score_b = evaluate(
        &net,
        &refresh(&net, &b, b.stm()),
        &refresh(&net, &b, !b.stm()),
    );

    assert_eq!(score_a, score_b, "mirrored positions disagreed");
}

// Swapping which accumulator is "us" must change the answer. If it does not,
// the two halves of output_weights are being read as one, and the perspective
// split -- the whole point of the architecture -- is not happening.
#[test]
fn perspective_order_matters() {
    if !net_available() {
        return;
    }

    let net = load_net(NET);
    let board = Board::from_fen("8/8/8/8/8/8/4P3/4K2k w - - 0 1").expect("bad FEN");

    let us = refresh(&net, &board, board.stm());
    let them = refresh(&net, &board, !board.stm());

    assert_ne!(
        evaluate(&net, &us, &them),
        evaluate(&net, &them, &us),
        "swapping perspectives changed nothing",
    );
}
