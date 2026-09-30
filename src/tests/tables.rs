use crate::tables::{BoundType, TransTable};
use crate::types::{Move, Score};

// Scores live in an i16, so the widest ones the search produces must survive the narrowing.
#[test]
fn store_probe_round_trip() {
    let tt = TransTable::new(1);

    let scores = [
        0, 1, -1,
        Score::MAX, -Score::MAX,
        Score::INF, -Score::INF,
        Score::NONE, -Score::NONE,
    ];
    let moves = [Move::NULL, Move::from_raw(0xffff), Move::from_raw(0x1234)];
    let depths = [0usize, 1, 7, 253, 254];
    let bounds = [BoundType::Exact, BoundType::Lower, BoundType::Upper];

// Every key is fresh, so each store lands in the cluster and a missing probe is a real bug.
    let mut key = 0x9e37_79b9_7f4a_7c15u64;
    for &depth in &depths {
        for &score in &scores {
            for &mv in &moves {
                for &bound in &bounds {
                    key = key
                        .wrapping_mul(6364136223846793005)
                        .wrapping_add(1442695040888963407);

                    tt.store(key, score, mv, depth, bound);
                    let (got_score, got_mv, got_depth, got_bound) =
                        tt.probe(key).expect("an entry just stored must probe back");

                    assert_eq!(got_score, score, "score, depth {depth}");
                    assert_eq!(got_mv, mv, "move, score {score}");
                    assert_eq!(got_depth, depth, "depth, score {score}");
                    assert_eq!(got_bound, bound, "bound, score {score}");
                }
            }
        }
    }
}

// Entries keep only the low 16 key bits; flipping one of them must miss.
#[test]
fn probe_rejects_wrong_key() {
    let tt = TransTable::new(1);
    let key = 0x0123_4567_89ab_cdefu64;

    tt.store(key, 123, Move::from_raw(0x1234), 9, BoundType::Exact);

    assert!(tt.probe(key).is_some(), "the stored key must hit");
    assert!(tt.probe(key ^ 1).is_none(), "a neighbouring key must not");
    assert!(tt.probe(!key).is_none(), "nor an unrelated one");
}

// hashfull has to read 0 when empty and near-full when saturated.
#[test]
fn hashfull_tracks_occupancy() {
    let tt = TransTable::new(1); // 1 MiB / 32 B = 32768 clusters of 3
    assert_eq!(tt.hashfull(), 0, "a fresh table reads empty");

    let mut key = 1u64;
    for _ in 0..200_000 {
        key = key
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        tt.store(key, 0, Move::NULL, 1, BoundType::Exact);
    }

    // 200k spread-out stores into 98k entries leaves nearly all of them written.
    let full = tt.hashfull();
    assert!(full > 500, "saturated table read {full} permille");
    assert!(full <= 1000, "permille out of range: {full}");

    tt.clear();
    assert_eq!(tt.hashfull(), 0, "clear resets the gauge");
}

// A fresh table must not hit on a key whose low 16 bits are zero.
#[test]
fn empty_entry_never_hits() {
    let tt = TransTable::new(1);
    assert!(tt.probe(0).is_none());
    assert!(tt.probe(0xabcd_0000_0000_0000).is_none());
}

// A shallower non-exact result for the same position keeps the deeper entry, but still refreshes its move.
#[test]
fn same_key_keeps_deeper_entry() {
    let tt = TransTable::new(1);
    let key = 0x0123_4567_89ab_cdefu64;

    tt.store(key, 50, Move::from_raw(0x1234), 12, BoundType::Lower);
    tt.store(key, -20, Move::from_raw(0x4321), 3, BoundType::Upper);

    let (score, mv, depth, bound) = tt.probe(key).unwrap();
    assert_eq!((score, depth, bound), (50, 12, BoundType::Lower));
    assert_eq!(mv, Move::from_raw(0x4321));

    tt.store(key, 7, Move::NULL, 2, BoundType::Exact);
    let (score, mv, depth, bound) = tt.probe(key).unwrap();
    assert_eq!((score, depth, bound), (7, 2, BoundType::Exact));
    assert_eq!(mv, Move::from_raw(0x4321), "a null move must not erase the stored one");
}
