// Accumulator refresh instrumentation. Compiled to nothing without `--features nnue-stats`.
//
//     cargo build --release --features nnue-stats
//     taskset -c 0 ./target/release/mythos bench
//
// Never measure NPS with it on -- the atomics sit in the refresh path.
//
//   refreshes per node       how often the expensive path fires; a wider KING_LAYOUT raises this.
//   features per refresh     the diff the Finny table actually applied.
//   full-rebuild equivalent  the piece count; its ratio to the above is the whole value of the table.
//   cold refreshes           first hit on a cell, where the diff degenerates to a full rebuild.

#[cfg(feature = "nnue-stats")]
mod imp {
    use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

    use crate::bench::group_digits;

    static REFRESHES: AtomicU64 = AtomicU64::new(0);
    static COLD: AtomicU64 = AtomicU64::new(0);
    static DIFF_FEATURES: AtomicU64 = AtomicU64::new(0);
    static FULL_FEATURES: AtomicU64 = AtomicU64::new(0);

    // `diff`: weight vectors this refresh touched. `full`: what a from-scratch rebuild would have, i.e. the piece count.
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
