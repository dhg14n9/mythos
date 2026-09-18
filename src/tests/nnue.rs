use std::path::Path;
use crate::board::board::Board;
use crate::nnue::{BUCKET_COUNT, BUCKET_SIZE, INPUT, NETWORK, OUTPUT_BUCKETS, QA};
use crate::nnue::accumulator::{feature_index, king_bucket, king_context, needs_refresh, should_mirror, AccState, Delta, FinnyTable};
use crate::nnue::network::{evaluate, forward, forward_scalar, load_net, materialize, push, refresh};
use crate::types::{Color, MoveList, Piece, PieceType, Square};

const NET: &str = env!("MYTHOS_NET");

const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

fn white(pt: PieceType) -> Piece {
    Piece::new(Color::White, pt)
}

fn black(pt: PieceType) -> Piece {
    Piece::new(Color::Black, pt)
}

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

    // mirrored: 5 * 64 + (4 ^ 7); E1 is kingside, so it folds onto D1
    assert_eq!(feature_index(Color::White, white(PieceType::King), Square::E1, true, 0), 323);

    // 0 * 64 + (48 ^ 7) + 384: the flip applies to the "them" half too
    assert_eq!(feature_index(Color::White, black(PieceType::Pawn), Square::A7, true, 0), 439);

    // bucket k shifts the same feature by k * 768
    assert_eq!(feature_index(Color::White, white(PieceType::Pawn), Square::A2, false, 1), 768 + 8);
    assert_eq!(feature_index(Color::White, white(PieceType::King), Square::E1, true, 9), 9 * 768 + 323);
}

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
                            assert_eq!(idx / BUCKET_SIZE, bucket, "index {idx} escaped bucket {bucket}");
                            assert!(!seen[idx], "index {idx} collided");
                            seen[idx] = true;
                        }
                    }
                }
            }

            assert!(seen.iter().all(|&s| s), "some inputs were never produced");
        }
    }
}

// KING_LAYOUT: four entries per rank from the king's own back rank, files a-d, e-h folded onto d-a.
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

// The net is gitignored, so skip rather than fail on a fresh clone.
fn net_available() -> bool {
    if Path::new(NET).exists() {
        return true;
    }
    eprintln!("skipping: {NET} not present (generate it first)");
    false
}

// Proves nothing overflows: debug builds panic on i16 overflow in `refresh`.
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

    // Sanity bound only; a random net has no opinion.
    assert!(score.abs() < 100_000, "implausible score {score}");
}

// The same position mirrored: ranks flipped and colours swapped, so the side to move sees an identical board.
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

// With pure HM a position and its file-mirror give byte-identical accumulators, so evals must be exactly equal.
const HM_PAIRS: &[(&str, &str)] = &[
    // kings on e1/e8 (both fold) against d1/d8 (neither does)
    (
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w - - 0 1",
        "rnbkqbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBKQBNR w - - 0 1",
    ),
    // White castled kingside (folds), Black queenside (does not): catches deciding both flips from one king
    (
        "2k4r/ppp5/8/8/8/8/5PPP/5RK1 w - - 0 1",
        "r4k2/5ppp/8/8/8/8/PPP5/1KR5 w - - 0 1",
    ),
    // Kiwipete mirrored: dense, asymmetric, every piece type on the board.
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

// Chosen so a two-ply walk hits every Delta shape: quiets, captures, en passant, both castles, promotions and capture-promotions.
const UPDATE_FENS: &[&str] = &[
    "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
    "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
    "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
    "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
    "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
    "n1n5/PPPk4/8/8/8/8/4Kppp/5N1N b - - 0 1",
    // Both kings on the d-file, so at depth 2 each colour crosses d->e; kiwipete only crosses the other way.
    "r5r1/2pk1p2/8/8/8/8/2PK1P2/R5R1 w - - 0 1",
    // A refresh that is also a capture: Kd1xe2 gives one add and two subs.
    "4k3/pp6/8/8/8/8/4n1PP/3K4 w - - 0 1",
];

// Independent derivation from the king's old and new squares rather than `needs_refresh`.
// `board` is the position AFTER the move; None when this colour's king never moved.
fn refresh_reason(board: &Board, delta: &Delta, color: Color) -> Option<(bool, bool)> {
    let king = Piece::new(color, PieceType::King);
    let from = delta.subs().iter().find(|&&(piece, _)| piece == king).map(|&(_, square)| square)?;

    let (new_mirror, new_bucket) = king_context(board, color);
    let old_mirror = from.is_kingside();

    Some((old_mirror != new_mirror, king_bucket(color, from, old_mirror) != new_bucket))
}

#[derive(Default)]
struct Refreshes {
    mirror_only: usize,
    bucket_only: usize,
    both: usize,
}

// Chain length in the lazy walk; roughly 30x the nodes per extra ply.
const WALK_DEPTH: usize = 2;

// Ply-0 root, built the way `Search::refresh_accumulators` does: both perspectives from scratch, marked computed.
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

// Re-defer plies 1..=ply so the next sibling leaf walks the full chain.
fn defer_line(stack: &mut [AccState], ply: usize) {
    for entry in stack[1..=ply].iter_mut() {
        for color in Color::ALL {
            // A refreshed entry stays computed: its delta crossed weight blocks and must never be replayed.
            if !needs_refresh(&entry.delta, color, entry.mirror[color], entry.bucket[color]) {
                entry.computed[color] = false;
            }
        }
    }
}

// lazy = false: materialise after every push, so every chain is one entry long.
// lazy = true: materialise only at the leaf, which exercises the walk back, the replay order and a mid-chain refresh.
fn walk_and_check(
    board: &mut Board,
    stack: &mut [AccState],
    ply: usize,
    depth: usize,
    fen: &str,
    refreshes: &mut Refreshes,
    lazy: bool,
    finny: &mut FinnyTable,
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

        // Delta reads the layout *before* the move; `push` wants the position after.
        let delta = Delta::new(board, mv);

        board.make_move(mv);
        push(&NETWORK, board, &mut stack[ply + 1], &delta, finny);

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

        walk_and_check(board, stack, ply + 1, depth - 1, fen, refreshes, lazy, finny);
        board.unmake_move(mv);
    }
}

fn run_walk(lazy: bool) -> Refreshes {
    let mut refreshes = Refreshes::default();

    // One table for the whole run: a fresh one makes every refresh cold, which tests nothing.
    let mut finny = FinnyTable::new(&NETWORK);

    for &fen in UPDATE_FENS {
        let mut board = Board::from_fen(fen).expect(fen);
        let mut stack = [AccState::empty(); WALK_DEPTH + 1];
        stack[0] = root_state(&board);

        walk_and_check(&mut board, &mut stack, 0, WALK_DEPTH, fen, &mut refreshes, lazy, &mut finny);
    }

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

// A refresh landing mid-chain: `push` marks that entry computed, so `materialize` must stop there.
#[test]
fn deferred_chain_matches_refresh() {
    run_walk(true);
}

// Only a WARM entry catches the bugs that live here -- a missing `entry.bb` writeback, a sign flip on the removed
// set, a dropped cell dimension -- so the second of the two passes is the one that counts.
#[test]
fn finny_refresh_matches_full_refresh() {
    let mut finny = FinnyTable::new(&NETWORK);

    let corpus: Vec<&str> = UPDATE_FENS
        .iter()
        .copied()
        .chain(HM_PAIRS.iter().flat_map(|&(a, b)| [a, b]))
        .chain([STARTPOS])
        .collect();

    // Which cells the corpus reached; a single-cell corpus would pass with either dimension deleted.
    let mut seen = [[[false; BUCKET_COUNT]; 2]; 2];

    for pass in 0..2 {
        for &fen in &corpus {
            let board = Board::from_fen(fen).expect(fen);

            for color in Color::ALL {
                let (mirror, bucket) = king_context(&board, color);
                seen[color][mirror as usize][bucket] = true;

                assert!(
                    finny.refresh(&NETWORK, &board, color, mirror, bucket)
                        == refresh(&NETWORK, &board, color),
                    "{fen}: pass {pass}: finny refresh != full refresh, {color} perspective",
                );
            }
        }
    }

    let cells = seen.iter().flatten().flatten().filter(|&&hit| hit).count();
    let mirrors = [false, true].map(|m| seen.iter().any(|p| p[m as usize].iter().any(|&hit| hit)));
    let perspectives = Color::ALL.map(|c| seen[c].iter().flatten().any(|&hit| hit));

    println!("finny cells exercised: {cells} of {}", 2 * 2 * BUCKET_COUNT);
    assert!(mirrors == [true, true], "the corpus never reached both mirror halves");
    assert!(perspectives == [true, true], "the corpus never reached both perspectives");
    assert!(cells > 2, "the corpus only reached {cells} cells -- warm reuse is not being tested");
}

// Pins the early return in `materialize`, without which a second call replays deltas already folded in.
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
    push(&NETWORK, &board, &mut stack[1], &delta, &mut FinnyTable::new(&NETWORK));

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

// An empty delta: move generation never produces it, but `negamax` does.
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
    push(&NETWORK, &board, &mut stack[1], &delta, &mut FinnyTable::new(&NETWORK));

    // ply 2: the null move, built the way search.rs builds it.
    stack[2].delta = Delta::empty();
    stack[2].computed = [false; 2];
    stack[2].mirror = stack[1].mirror;
    stack[2].bucket = stack[1].bucket;

    // A null move leaves the piece layout alone, so ply 2 must equal a refresh at ply 1.
    check_against_refresh(&board, &mut stack, 2, fen, "null move");
}

// AVX2 reassociates screlu(x) * w into x * (x * w) with the middle term in i16, valid only while QA * max|w| fits.
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

// Must be bit-exact, not close: a mismatch of one can cross a quantisation boundary.
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
