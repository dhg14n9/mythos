// Instrumentation for the accumulator refresh path. Off by default and
// compiled to nothing: without `--features nnue-stats` every entry point below
// is an empty inline fn, so the shipped binary and every SPRT build carry no
// counters and no atomics.
//
//     cargo build --release --features nnue-stats
//     taskset -c 0 ./target/release/mythos bench
//
// Never measure NPS with it enabled -- the atomics are relaxed and cheap, but
// they are not free and they sit in the refresh path.
//
// What the four numbers are for:
//
//   refreshes per node       how often the expensive path fires at all. This is
//                            the figure a wider KING_LAYOUT raises, and the
//                            reason to read it before widening one.
//   features per refresh     the diff the Finny table actually applied.
//   full-rebuild equivalent  what that same refresh would have cost starting
//                            from feature_bias -- the piece count. The ratio of
//                            these two is the entire value of the table, and it
//                            is the honest measure of this change in a way that
//                            an NPS delta is not: NPS also carries whatever
//                            else moved in the same window.
//   cold refreshes           first hit on a cell, where the diff degenerates to
//                            a full rebuild. Negligible today; it grows with the
//                            number of cells, which is the second thing a wider
//                            layout costs and the one that is easy to forget.

#[cfg(feature = "nnue-stats")]
mod imp {
    use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

    use crate::bench::group_digits;

    static REFRESHES: AtomicU64 = AtomicU64::new(0);
    static COLD: AtomicU64 = AtomicU64::new(0);
    static DIFF_FEATURES: AtomicU64 = AtomicU64::new(0);
    static FULL_FEATURES: AtomicU64 = AtomicU64::new(0);

    // `diff` is the number of weight vectors this refresh added or subtracted;
    // `full` is how many a from-scratch rebuild would have touched, i.e. the
    // piece count. `cold` marks an entry whose snapshot was still empty, in
    // which case the two are equal by construction.
    pub fn record_refresh(diff: u64, full: u64, cold: bool) {
        REFRESHES.fetch_add(1, Relaxed);
        DIFF_FEATURES.fetch_add(diff, Relaxed);
        FULL_FEATURES.fetch_add(full, Relaxed);
        if cold {
            COLD.fetch_add(1, Relaxed);
        }
    }

    pub fn reset() {
        for counter in [&REFRESHES, &COLD, &DIFF_FEATURES, &FULL_FEATURES] {
            counter.store(0, Relaxed);
        }
    }

    pub fn report(nodes: u64) {
        let refreshes = REFRESHES.load(Relaxed);
        let cold = COLD.load(Relaxed);
        let diff = DIFF_FEATURES.load(Relaxed);
        let full = FULL_FEATURES.load(Relaxed);

        let per = |n: u64, d: u64| if d == 0 { 0.0 } else { n as f64 / d as f64 };

        println!();
        println!("  -- finny tables (nnue-stats) --");
        println!("  refreshes      : {:>13}  ({:.2} per 100 nodes)",
                 group_digits(refreshes), per(refreshes, nodes) * 100.0);
        println!("  cold           : {:>13}  ({:.3}% of refreshes)",
                 group_digits(cold), per(cold, refreshes) * 100.0);
        println!("  features       : {:>13}  ({:.1} per refresh)",
                 group_digits(diff), per(diff, refreshes));
        println!("  full rebuild   : {:>13}  ({:.1} per refresh)",
                 group_digits(full), per(full, refreshes));
        println!("  saved          : {:>12.1}%", (1.0 - per(diff, full)) * 100.0);
    }
}

#[cfg(not(feature = "nnue-stats"))]
mod imp {
    #[inline(always)]
    pub fn record_refresh(_diff: u64, _full: u64, _cold: bool) {}

    #[inline(always)]
    pub fn reset() {}

    #[inline(always)]
    pub fn report(_nodes: u64) {}
}

pub use imp::{record_refresh, report, reset};
