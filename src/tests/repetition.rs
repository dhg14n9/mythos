use crate::board::board::Board;
use crate::types::MoveList;

fn play(board: &mut Board, moves: &str) {
    for uci in moves.split_whitespace() {
        if uci == "null" {
            board.make_null_move();
            continue;
        }
        let mut list = MoveList::new();
        board.gen_move(&mut list, false);
        let mv = (0..list.len())
            .map(|i| list.get_nth(i))
            .find(|mv| mv.to_string() == uci)
            .unwrap_or_else(|| panic!("no legal move {uci}"));
        board.make_move(mv);
    }
}

const SHUFFLE: &str = "g1f3 g8f6 f3g1 f6g8";

// The start position has now occurred twice. That isn't a draw at game level,
// and not in search either when the earlier copy is the root itself.
#[test]
fn twofold_at_or_before_root_is_not_draw() {
    let mut board = Board::start_pos();
    play(&mut board, SHUFFLE);
    assert!(!board.is_draw(0));
    assert!(!board.is_draw(4)); // root = start position, 4 plies in
}

#[test]
fn threefold_is_draw_anywhere() {
    let mut board = Board::start_pos();
    play(&mut board, SHUFFLE);
    play(&mut board, SHUFFLE);
    assert!(board.is_draw(0));
}

// Root = start position. The position after g1f3 occurs at ply 1 and ply 5, both after the root.
#[test]
fn in_tree_twofold_is_draw() {
    let mut board = Board::start_pos();
    play(&mut board, SHUFFLE);
    play(&mut board, "g1f3");
    assert!(board.is_draw(5));
    assert!(!board.is_draw(0));
}

// White wastes no tempo, black triangulates, and white's pass is what closes the cycle.
#[test]
fn no_repetition_through_null_move() {
    let mut board = Board::from_fen("4k3/8/8/8/8/8/8/4K3 w - - 0 1").unwrap();
    play(&mut board, "e1f1 e8e7 null e7f8 f1e1");
    assert!(!board.upcoming_reps(100), "black's f8e8 only repeats thanks to the pass");
    play(&mut board, "f8e8");
    assert!(!board.is_draw(100));
}

#[test]
fn upcoming_reps_matches_is_draw() {
    // Root = start position; white to move at ply 4 can replay g1f3, whose position is at ply 1.
    let mut board = Board::start_pos();
    play(&mut board, SHUFFLE);
    assert!(board.upcoming_reps(4));
    assert!(!board.upcoming_reps(0), "position after g1f3 occurred once, before the root");

    // Black to move at ply 3 can play f6g8 back to the start position, which is the root: not a draw.
    let mut board = Board::start_pos();
    play(&mut board, "g1f3 g8f6 f3g1");
    assert!(!board.upcoming_reps(3));

    // Once the start position has already repeated, going back to it is a threefold at any ply.
    let mut board = Board::start_pos();
    play(&mut board, SHUFFLE);
    play(&mut board, "g1f3 g8f6 f3g1");
    assert!(board.upcoming_reps(0));
}
