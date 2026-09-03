use std::path::Path;
use crate::board::board::Board;
use crate::nnue::{BUCKET_COUNT, BUCKET_SIZE, INPUT, NETWORK, OUTPUT_BUCKETS, QA};
use crate::nnue::accumulator::{feature_index, king_bucket, king_context, needs_refresh, should_mirror, AccState, Delta};
use crate::nnue::network::{evaluate, forward, forward_scalar, load_net, materialize, push, refresh};
use crate::types::{Color, MoveList, Piece, PieceType, Square};

const NET: &str = "nets/net.nnue";

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
    assert_eq!(feature_index(Color::White, white(PieceType::Pawn), Square::A2, false, 0), 8);

    // "them" AND mirrored: 0 * 64 + (8 ^ 56) + 384
    assert_eq!(feature_index(Color::Black, white(PieceType::Pawn), Square::A2, false, 0), 432);

    // "them", no mirror: 0 * 64 + 48 + 384
    assert_eq!(feature_index(Color::White, black(PieceType::Pawn), Square::A7, false, 0), 432);

    // pins the piece_type stride at King = 5: 5 * 64 + 4 + 0
    assert_eq!(feature_index(Color::White, white(PieceType::King), Square::E1, false, 0), 324);

    // The same king, mirrored: 5 * 64 + (4 ^ 7) + 0. E1 is on the kingside, so
    // it is E1 that gets folded, and it lands on D1 -- this is the assertion
    // that says which direction flip_file goes.
    assert_eq!(feature_index(Color::White, white(PieceType::King), Square::E1, true, 0), 323);

    // The flip applies to the "them" half too, not just to our own pieces:
    // 0 * 64 + (48 ^ 7) + 384, i.e. a black pawn on A7 folds onto H7.
    assert_eq!(feature_index(Color::White, black(PieceType::Pawn), Square::A7, true, 0), 439);

    // The king-bucket stride: a bucket is a whole 768-input block, so the same
    // feature in bucket k sits exactly k * 768 further along. The engine's
    // block ordering has to match bullet's, which lays the buckets out
    // contiguously and repeats the factoriser across them on save.
    assert_eq!(feature_index(Color::White, white(PieceType::Pawn), Square::A2, false, 1), 768 + 8);
    assert_eq!(feature_index(Color::White, white(PieceType::King), Square::E1, true, 9), 9 * 768 + 323);
}

// "A white piece on square s, seen by Black" and "a black piece of the same
// type on the mirrored square, seen by White" are the SAME situation, so they
// must map to the same input. This holds only if both flips -- the colour
// offset and the rank mirror -- are right; breaking either one breaks it.
//
// Run for both mirror settings: the file flip and the colour/rank flip touch
// disjoint bits, so horizontal mirroring must leave this property untouched.
#[test]
fn feature_index_is_colour_mirror_symmetric() {
    for bucket in 0..BUCKET_COUNT {
        for mirror in [false, true] {
            for pt in PieceType::ALL {
                for sq in 0..64u8 {
                    let square = Square::new(sq);
                    let mirrored = square.flip_rank();

                    assert_eq!(
                        feature_index(Color::Black, white(pt), square, mirror, bucket),
                        feature_index(Color::White, black(pt), mirrored, mirror, bucket),
                        "{pt} on {square} broke mirror symmetry (mirror = {mirror}, bucket = {bucket})",
                    );
                }
            }
        }
    }
}

// Every (perspective, piece, square) must land inside the 768 inputs, and no
// two distinct (piece, square) pairs may collide from one perspective -- a
// collision would silently merge two features into one.
#[test]
fn feature_index_is_in_range_and_injective() {
    for mirror in [false, true] {
        for perspective in Color::ALL {
            let mut seen = [false; INPUT];

            for bucket in 0..BUCKET_COUNT {
                for colour in Color::ALL {
                    for pt in PieceType::ALL {
                        for sq in 0..64u8 {
                            let idx = feature_index(perspective, Piece::new(colour, pt), Square::new(sq), mirror, bucket);

                            assert!(idx < INPUT, "index {idx} out of range");
                            // Pins the stride: a bucket's features must stay
                            // inside that bucket's own block of weights.
                            assert_eq!(idx / BUCKET_SIZE, bucket, "index {idx} escaped bucket {bucket}");
                            assert!(!seen[idx], "index {idx} collided");
                            seen[idx] = true;
                        }
                    }
                }
            }

            // 2 colours * 6 types * 64 squares == 768 per bucket, so every input
            // is claimed. flip_file is a bijection on squares, so this must hold
            // mirrored too.
            assert!(seen.iter().all(|&s| s), "some inputs were never produced");
        }
    }
}

// ---------------------------------------------------------------- king buckets

// The anchors that fix the ORIENTATION of KING_LAYOUT, hand-computed from the
// trainer's array. Nothing else can catch a layout read from the wrong end: a
// rank order that is upside down, or a fold that runs the wrong way, is wrong
// identically for both perspectives, so every symmetry property below still
// holds and only the strength is quietly gone.
//
// KING_LAYOUT is four entries per rank, starting from the king's OWN back rank,
// files a-d, with e-h folded onto d-a.
#[test]
fn king_bucket_hand_computed() {
    let cases = [
        (Color::White, Square::A1, 0), // the corner: rank 0, file 0
        (Color::White, Square::H1, 0), // ... and its fold, h -> a
        (Color::White, Square::G1, 1), // castled kingside, folds onto B1
        (Color::White, Square::C1, 2), // queenside, no fold
        (Color::White, Square::E1, 3), // folds onto D1: rank 0, file 3
        (Color::White, Square::D1, 3), // ... which D1 reaches without folding
        (Color::White, Square::E4, 7), // rank 3, file 3 -> layout[15]
        (Color::White, Square::E8, 9), // white king on the far rank -> layout[31]
        (Color::Black, Square::E8, 3), // the same square is Black's own E1
        (Color::Black, Square::G8, 1),
        (Color::Black, Square::E5, 7), // Black's rank 3
    ];

    for (perspective, king, expected) in cases {
        assert_eq!(
            king_bucket(perspective, king, king.is_kingside()),
            expected,
            "{perspective} king on {king}",
        );
    }
}

// A position and its colour-mirror must pick the same bucket, or the two
// perspectives are reading different halves of the layout.
#[test]
fn king_bucket_is_perspective_symmetric() {
    for sq in 0..64u8 {
        let white = Square::new(sq);
        let black = white.flip_rank();

        assert_eq!(
            king_bucket(Color::White, white, white.is_kingside()),
            king_bucket(Color::Black, black, black.is_kingside()),
            "{white} seen by White and {black} seen by Black disagreed",
        );
    }
}

// Horizontal mirroring happens before the layout lookup, so a square and its
// file-mirror are the same bucket. This is what makes the bucket safe to leave
// out of the HM equivalence test below.
#[test]
fn king_bucket_folds_across_the_file() {
    for perspective in Color::ALL {
        for sq in 0..64u8 {
            let square = Square::new(sq);
            let folded = square.flip_file();

            assert_eq!(
                king_bucket(perspective, square, square.is_kingside()),
                king_bucket(perspective, folded, folded.is_kingside()),
                "{square} and {folded} landed in different buckets",
            );
        }
    }
}

// king_context is the single lookup that push, refresh and the root all go
// through. It must agree with the two functions it replaced -- and in
// particular it must read each colour's OWN king, which is the mistake the
// helper exists to make impossible.
#[test]
fn king_context_agrees_with_its_parts() {
    for &fen in UPDATE_FENS {
        let board = Board::from_fen(fen).expect(fen);

        for color in Color::ALL {
            let king = board.piece_bb(Piece::new(color, PieceType::King)).lsb();
            let (mirror, bucket) = king_context(&board, color);

            assert_eq!(mirror, should_mirror(&board, color), "{fen}: {color} mirror");
            assert_eq!(bucket, king_bucket(color, king, mirror), "{fen}: {color} bucket");
        }
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

    let score = evaluate(&net, &us, &them, 0);
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
        0,
    );
    let score_b = evaluate(
        &net,
        &refresh(&net, &b, b.stm()),
        &refresh(&net, &b, !b.stm()),
        0,
    );

    assert_eq!(score_a, score_b, "mirrored positions disagreed");
}

// The strongest check on horizontal mirroring, because it tests a *property*
// rather than a hand-computed constant: with pure HM and no other asymmetry in
// the feature set, a position and its file-mirror produce byte-identical
// accumulators, so their evals must be exactly equal.
//
// Uses NETWORK rather than load_net: the property holds for any weights at all,
// so there is nothing to skip on and no reason to guard.
//
// Castling rights and en passant are '-' in every pair. Neither is an input
// feature, and mirroring the board swaps which rook is which, so carrying them
// would only invite a pointless argument about the FEN.
const HM_PAIRS: &[(&str, &str)] = &[
    // Startpos and its mirror: kings on e1/e8 (kingside, so BOTH perspectives
    // fold) against kings on d1/d8 (queenside, so NEITHER does). A full board of
    // material, and the two positions sit on opposite sides of the flag.
    (
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w - - 0 1",
        "rnbkqbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBKQBNR w - - 0 1",
    ),
    // THE ONE THAT MATTERS. White is castled kingside (Kg1, folds) while Black
    // sits queenside (Kc8, does not) -- one accumulator mirrored and the other
    // not, in the same position. In the mirror the roles swap. This is what
    // catches the most likely bug in the whole change: deciding both
    // perspectives' flip from one shared king instead of each from its own.
    (
        "2k4r/ppp5/8/8/8/8/5PPP/5RK1 w - - 0 1",
        "r4k2/5ppp/8/8/8/8/PPP5/1KR5 w - - 0 1",
    ),
    // Kiwipete mirrored: dense, asymmetric, every piece type on the board, so a
    // flip that is right for kings and pawns but wrong for some other stride
    // has nowhere to hide.
    (
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w - - 0 1",
        "r2k3r/1bpqpp1p/1pnp2nb/3NP3/3P2p1/p1Q2N2/PPPBBPPP/R2K3R w - - 0 1",
    ),
];

#[test]
fn horizontally_mirrored_positions_evaluate_identically() {
    for &(left, right) in HM_PAIRS {
        let a = Board::from_fen(left).expect(left);
        let b = Board::from_fen(right).expect(right);

        let score_a = evaluate(
            &NETWORK,
            &refresh(&NETWORK, &a, a.stm()),
            &refresh(&NETWORK, &a, !a.stm()),
            0,
        );
        let score_b = evaluate(
            &NETWORK,
            &refresh(&NETWORK, &b, b.stm()),
            &refresh(&NETWORK, &b, !b.stm()),
            0,
        );

        assert_eq!(score_a, score_b, "{left} and its mirror {right} disagreed");
    }
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
        evaluate(&net, &us, &them, 0),
        evaluate(&net, &them, &us, 0),
        "swapping perspectives changed nothing",
    );
}

// ---------------------------------------------------------------- incremental update

// Chosen so that a two-ply walk hits every Delta shape there is: quiet moves,
// captures, en passant, castling both sides, plain promotions and
// capture-promotions.
const UPDATE_FENS: &[&str] = &[
    "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
    "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
    "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
    "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
    "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
    "n1n5/PPPk4/8/8/8/8/4Kppp/5N1N b - - 0 1",
    // Both kings on the d-file with material either side of them, so at depth 2
    // each colour crosses d->e (queenside to kingside) in several ways. The
    // Kiwipete entry above only ever crosses in the other direction, and a sign
    // error in the comparison would pass one and fail the other.
    "r5r1/2pk1p2/8/8/8/8/2PK1P2/R5R1 w - - 0 1",
    // A refresh that is also a capture: Kd1xe2 leaves a delta with one add and
    // two subs, so it checks that the refresh replaces the set_add_sub2 shape
    // and not just the plain one.
    "4k3/pp6/8/8/8/8/4n1PP/3K4 w - - 0 1",
];

// Why a `push` refreshed, recomputed here from the king's old and new squares
// rather than by asking `needs_refresh`. The point is to have a second,
// independent derivation of the same fact: the walk asserts the two agree, so
// the counting below cannot drift into merely restating the implementation.
//
// `board` is the position AFTER the move. Returns None when the move did not
// touch this colour's king, in which case no refresh may happen at all.
fn refresh_reason(board: &Board, delta: &Delta, color: Color) -> Option<(bool, bool)> {
    let king = Piece::new(color, PieceType::King);
    let from = delta.subs().iter().find(|&&(piece, _)| piece == king).map(|&(_, square)| square)?;

    let (new_mirror, new_bucket) = king_context(board, color);
    let old_mirror = from.is_kingside();

    Some((old_mirror != new_mirror, king_bucket(color, from, old_mirror) != new_bucket))
}

// The two refresh triggers, counted separately. They are genuinely independent:
// Ke1-d1 flips the mirror while staying in bucket 3, and Kd1-c1 changes bucket
// with no flip, so an implementation that checks only one of the two conditions
// passes half these counters and fails the other half.
#[derive(Default)]
struct Refreshes {
    mirror_only: usize,
    bucket_only: usize,
    both: usize,
}

// How deep the walks go. Chains of deferred entries are exactly this long in
// the lazy walk, so raising it widens the replay coverage -- at roughly a 30x
// cost per ply, which is why it is not higher.
const WALK_DEPTH: usize = 2;

// The root of a walk, built the way `Search::refresh_accumulators` builds ply 0:
// both perspectives from scratch and marked computed. That flag is what
// terminates every walk back, so a test stack without it would either panic in
// debug or index wildly in release.
fn root_state(board: &Board) -> AccState {
    let mut state = AccState::empty();

    state.accs[0] = refresh(&NETWORK, board, Color::White);
    state.accs[1] = refresh(&NETWORK, board, Color::Black);
    state.computed = [true; 2];

    for color in Color::ALL {
        let (mirror, bucket) = king_context(board, color);
        state.mirror[color] = mirror;
        state.bucket[color] = bucket;
    }

    state
}

// Materialise both perspectives at `ply` and compare against a from-scratch
// refresh of the position the board is actually in.
fn check_against_refresh(board: &Board, stack: &mut [AccState], ply: usize, fen: &str, mode: &str) {
    for color in Color::ALL {
        materialize(&NETWORK, stack, ply, color);

        assert!(
            stack[ply].computed[color],
            "{fen}: {mode}: materialize left {color} uncomputed at ply {ply}",
        );
        assert!(
            stack[ply].accs[color] == refresh(&NETWORK, board, color),
            "{fen}: {mode}: incremental != refresh at ply {ply}, {color} perspective",
        );
    }
}

// Mark plies 1..=ply deferred again, without touching their deltas.
//
// Only used by the lazy walk, and only to preserve its coverage: materialising
// at a leaf marks every entry in that chain computed, so the *next* sibling
// leaf would walk back only one ply and the long-chain case would be tested
// once per subtree instead of once per leaf. Re-deferring restores the state
// the line was in before anything read it, which is a state the real search is
// in constantly.
fn defer_line(stack: &mut [AccState], ply: usize) {
    for entry in stack[1..=ply].iter_mut() {
        for color in Color::ALL {
            // Only re-defer what `push` would have deferred. An entry it
            // refreshed must stay computed: its delta moved the king to a
            // different weight block, which is precisely the delta that must
            // never be replayed incrementally.
            if !needs_refresh(&entry.delta, color, entry.mirror[color], entry.bucket[color]) {
                entry.computed[color] = false;
            }
        }
    }
}

// The incremental path has to land on exactly what a from-scratch refresh
// would. Its failure mode is not a crash: it is an eval that is quietly wrong
// in whichever rare position the broken shape occurs in, which just bleeds Elo.
//
// Both perspectives are checked every time, because a mistake in the "them"
// (+384) half of feature_index shows up on one side only.
//
// `lazy` picks which half of the deferred-update contract is under test:
//
//   false -- materialise immediately after every push, so every chain is one
//            entry long. This is the old eager `update` path and it isolates
//            the delta shapes and the mirroring flag.
//   true  -- push all the way down and materialise only at the leaf, so the
//            chain is WALK_DEPTH entries long. This is the only mode that
//            exercises the walk back, the replay order, and a refresh sitting
//            in the middle of a chain.
//
// The lazy mode also covers the stale-slot family of bugs for free: siblings at
// one ply share a slot, so a `push` that failed to overwrite `delta` or
// `computed` is caught the moment the second sibling materialises.
//
// `refreshes` counts how many times each refresh trigger actually fired, so the
// caller can assert the corpus reaches all of them. Without that the whole test
// can quietly stop covering a trigger -- reorder a FEN or change the depth and
// the king moves that reach it vanish while the suite stays green.
fn walk_and_check(
    board: &mut Board,
    stack: &mut [AccState],
    ply: usize,
    depth: usize,
    fen: &str,
    refreshes: &mut Refreshes,
    lazy: bool,
) {
    if depth == 0 {
        if lazy {
            check_against_refresh(board, stack, ply, fen, "lazy leaf");
            defer_line(stack, ply);
        }
        return;
    }

    let mut list = MoveList::new();
    board.gen_move(&mut list, false);

    for i in 0..list.len() {
        let mv = list.get_nth(i);

        // Delta reads the piece layout as it stands *before* the move is played,
        // but `push` wants the position *after* it -- the mirroring flag and
        // the bucket are both read off the new king square.
        let delta = Delta::new(board, mv);

        board.make_move(mv);
        push(&NETWORK, board, &mut stack[ply + 1], &delta);

        for color in Color::ALL {
            let (mirror, bucket) = king_context(board, color);
            let fired = needs_refresh(&delta, color, mirror, bucket);

            match refresh_reason(board, &delta, color) {
                None => assert!(
                    !fired,
                    "{fen}: needs_refresh fired for {color} on a move that never touched its king",
                ),
                Some((moved_mirror, moved_bucket)) => {
                    assert_eq!(
                        fired,
                        moved_mirror || moved_bucket,
                        "{fen}: needs_refresh disagreed with the king's own context for {color}",
                    );

                    match (moved_mirror, moved_bucket) {
                        (true, false) => refreshes.mirror_only += 1,
                        (false, true) => refreshes.bucket_only += 1,
                        (true, true) => refreshes.both += 1,
                        (false, false) => {}
                    }
                }
            }
        }

        if !lazy {
            check_against_refresh(board, stack, ply + 1, fen, "eager");
        }

        walk_and_check(board, stack, ply + 1, depth - 1, fen, refreshes, lazy);
        board.unmake_move(mv);
    }
}

fn run_walk(lazy: bool) -> Refreshes {
    let mut refreshes = Refreshes::default();

    for &fen in UPDATE_FENS {
        let mut board = Board::from_fen(fen).expect(fen);
        let mut stack = [AccState::empty(); WALK_DEPTH + 1];
        stack[0] = root_state(&board);

        walk_and_check(&mut board, &mut stack, 0, WALK_DEPTH, fen, &mut refreshes, lazy);
    }

    // The assertion inside the walk is vacuous for the refresh path unless the
    // walk actually reaches it, and reaching it by one trigger says nothing
    // about the other. Pin all three shapes, and print them so a drop is
    // visible when the corpus changes.
    println!(
        "refreshes exercised: mirror only {}, bucket only {}, both {}",
        refreshes.mirror_only, refreshes.bucket_only, refreshes.both,
    );
    assert!(refreshes.mirror_only > 0, "no king crossed d/e without changing bucket");
    assert!(refreshes.bucket_only > 0, "no king changed bucket without crossing d/e");
    assert!(refreshes.both > 0, "no king changed both at once");

    refreshes
}

#[test]
fn incremental_update_matches_refresh() {
    run_walk(false);
}

// The deferred half. A refresh landing mid-chain is the case that decides
// whether the walk back is right: `push` refreshes that entry on the spot and
// marks it computed, so `materialize` must stop there rather than replaying the
// king move as an ordinary delta on top of it. Get that wrong and the result is
// not a panic, it is a plausible-looking wrong accumulator -- which is why this
// test asserts the corpus reaches a refresh rather than trusting it to.
#[test]
fn deferred_chain_matches_refresh() {
    run_walk(true);
}

// Reading an accumulator must not change it. This pins the early return in
// `materialize`: without it a second call walks back past an already-computed
// entry and replays deltas that are already folded in.
#[test]
fn materialize_is_idempotent() {
    let fen = UPDATE_FENS[1];
    let mut board = Board::from_fen(fen).expect(fen);
    let mut stack = [AccState::empty(); WALK_DEPTH + 1];
    stack[0] = root_state(&board);

    let mut list = MoveList::new();
    board.gen_move(&mut list, false);
    let mv = list.get_nth(0);

    let delta = Delta::new(&board, mv);
    board.make_move(mv);
    push(&NETWORK, &board, &mut stack[1], &delta);

    for color in Color::ALL {
        materialize(&NETWORK, &mut stack, 1, color);
        let once = stack[1].accs[color];

        materialize(&NETWORK, &mut stack, 1, color);
        assert!(
            stack[1].accs[color] == once,
            "a second materialize changed the {color} accumulator",
        );
    }
}

// The null-move shape: an entry whose delta is empty, so its accumulator is its
// parent's unchanged. Move generation never produces it, so the `([], [])` arm
// and the null-move path in `negamax` are untested unless it is built by hand --
// which is exactly how the search builds it.
#[test]
fn empty_delta_copies_the_parent() {
    let fen = UPDATE_FENS[1];
    let mut board = Board::from_fen(fen).expect(fen);
    let mut stack = [AccState::empty(); WALK_DEPTH + 1];
    stack[0] = root_state(&board);

    // ply 1: a real move, left deferred so the empty entry sits mid-chain.
    let mut list = MoveList::new();
    board.gen_move(&mut list, false);
    let mv = list.get_nth(0);

    let delta = Delta::new(&board, mv);
    board.make_move(mv);
    push(&NETWORK, &board, &mut stack[1], &delta);

    // ply 2: the null move, built the way search.rs builds it.
    stack[2].delta = Delta::empty();
    stack[2].computed = [false; 2];
    stack[2].mirror = stack[1].mirror;
    stack[2].bucket = stack[1].bucket;

    // A null move leaves the piece layout alone, so the board is still the
    // position at ply 1 and a refresh of it is what ply 2 must equal.
    check_against_refresh(&board, &mut stack, 2, fen, "null move");
}

// ---------------------------------------------------------------- simd forward pass

// The AVX2 forward pass reassociates screlu(x) * w into x * (x * w) and holds
// the middle term in an i16. That is only valid while QA * max|w| fits, so pin
// the invariant against the net that is actually compiled in -- a retrain with
// different quantisation is exactly the change that would silently break it.
#[test]
fn output_weights_fit_in_i16() {
    let worst = (0..OUTPUT_BUCKETS)
        .flat_map(|bucket| NETWORK.output_weights(bucket).iter())
        .map(|w| w.unsigned_abs())
        .max()
        .unwrap();
    let product = i32::from(QA) * i32::from(worst);

    println!("max |output_weight| = {worst}, QA * it = {product}");
    assert!(
        product <= i32::from(i16::MAX),
        "QA ({QA}) * max |output_weight| ({worst}) = {product} overflows i16 -- \
         the AVX2 forward pass in network.rs is no longer valid for this net",
    );
}

// The SIMD path has to be bit-exact against the scalar one, not merely close:
// a mismatch of one in the raw sum can cross a quantisation boundary and change
// the search tree. Runs over every FEN in this file so the accumulators carry
// realistic, and in places extreme, values.
#[test]
fn simd_forward_matches_scalar() {
    let fens = UPDATE_FENS
        .iter()
        .copied()
        .chain(HM_PAIRS.iter().flat_map(|&(a, b)| [a, b]))
        .chain([STARTPOS, "8/8/8/8/8/8/4P3/4K2k w - - 0 1"]);

    for fen in fens {
        let board = Board::from_fen(fen).expect(fen);
        let us = refresh(&NETWORK, &board, board.stm());
        let them = refresh(&NETWORK, &board, !board.stm());

        for bucket in 0..OUTPUT_BUCKETS {
            assert_eq!(
                forward(&NETWORK, &us, &them, bucket),
                forward_scalar(&NETWORK, &us, &them, bucket),
                "{fen}: simd forward pass disagreed with the scalar one in bucket {bucket}",
            );
        }
    }
}
