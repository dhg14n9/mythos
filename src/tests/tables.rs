use crate::tables::{BoundType, TransTable};
use crate::types::{Move, Score};

// A single u64 carries score, move, depth and bound, so nothing in the type
// system notices when a field is shifted or masked wrong. The only guard is
// that every value survives a store/probe round trip. Negative scores are the
// case that matters: recovering them needs an arithmetic shift, and a logical
// one turns them into large positives -- which the search happily believes.
#[test]
fn store_probe_round_trip() {
    let tt = TransTable::new(1);

    let scores = [
        0, 1, -1,
        Score::MAX, -Score::MAX,
        Score::INF, -Score::INF,
        Score::NONE, -Score::NONE,
        (1 << 17) - 1, -(1 << 17), // the widest values the field can hold
    ];
    let moves = [Move::NULL, Move::from_raw(0xffff), Move::from_raw(0x1234)];
    let depths = [0usize, 1, 7, 254, 255];
    let bounds = [BoundType::Exact, BoundType::Lower, BoundType::Upper];

    // store is depth-preferred, so a write can be *rejected* when two keys collide
    // in the same slot. Walking depth ascending on the outside guarantees any entry
    // already sitting there is no deeper than the one going in, so every store wins
    // and a missing probe still means a genuine packing bug.
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

// The table is lockless: the slot holds `key ^ data`, so a torn or unrelated
// entry fails the XOR check instead of being handed back as this position's.
#[test]
fn probe_rejects_wrong_key() {
    let tt = TransTable::new(1);
    let key = 0x0123_4567_89ab_cdefu64;

    tt.store(key, 123, Move::from_raw(0x1234), 9, BoundType::Exact);

    assert!(tt.probe(key).is_some(), "the stored key must hit");
    assert!(tt.probe(key ^ 1).is_none(), "a neighbouring key must not");
    assert!(tt.probe(!key).is_none(), "nor an unrelated one");
}

// hashfull is what tells us whether a bench run is actually stressing the
// table, so it has to read 0 when empty and near-full when saturated -- a gauge
// stuck at either end would quietly invalidate every replacement measurement.
#[test]
fn hashfull_tracks_occupancy() {
    let tt = TransTable::new(1); // 1 MiB / 16 B = 65536 slots
    assert_eq!(tt.hashfull(), 0, "a fresh table reads empty");

    let mut key = 1u64;
    for _ in 0..200_000 {
        key = key
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        tt.store(key, 0, Move::NULL, 1, BoundType::Exact);
    }

    // 200k spread-out stores into 65k slots leaves ~95% of them written.
    let full = tt.hashfull();
    assert!(full > 500, "saturated table read {full} permille");
    assert!(full <= 1000, "permille out of range: {full}");

    tt.clear();
    assert_eq!(tt.hashfull(), 0, "clear resets the gauge");
}
