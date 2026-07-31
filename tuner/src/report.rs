use std::fmt::Write as _;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use mythos::eval::trace::{initial_weights, BISHOP_MOB, BISHOP_PAIR, DOUBLED_PAWN, ISOLATED_PAWN,
    KNIGHT_MOB, NUM_PARAMS, PASSED_PAWN, PSQT, QUEEN_MOB, ROOK_MOB, TEMPO};

use crate::format::unpack;
use crate::train::Weights;

// The tables are the point of this file, but a table on its own cannot be read
// critically: 617 in QUEEN_MOB[27] looks like a tuned value whether the parameter
// fired 1.4M times or 279, and whether the run had settled or was still swinging
// ±1cp per epoch. Everything above the tables answers "should I believe this
// number", and makes two runs comparable without re-running either.
pub struct Run {
    pub epochs: usize,
    pub k: f64,
    pub lr: f64,

    pub records: usize,
    pub coeffs: usize,
    pub labels: [u64; 3],
    pub train_len: usize,
    pub val_len: usize,

    pub baseline: f64,
    pub initial: (f64, f64), // (train, val) before the first step
    pub last: (f64, f64),    // (train, val) at the final epoch
    pub trace: Vec<Epoch>,

    pub t_open: Duration,
    pub t_fit_k: Duration,
    pub t_train: Duration,
    pub t_diag: Duration,

    pub gradient: GradStats,
    pub fired: Vec<u32>
}

pub struct Epoch {
    pub epoch: usize,
    pub train: f64,
    pub val: f64,
    pub churn: usize
}

pub struct GradStats {
    max: f64,
    at: (usize, usize),
    norm: f64
}

impl GradStats {
    pub fn of(gradient: &Weights) -> Self {
        let mut max = 0f64;
        let mut at = (0, 0);
        let mut sum = 0f64;

        for i in 0..NUM_PARAMS {
            for half in 0..2 {
                let g = gradient[i][half];
                sum += g * g;

                if g.abs() > max {
                    max = g.abs();
                    at = (i, half);
                }
            }
        }

        Self { max, at, norm: sum.sqrt() }
    }
}

// The loss cannot see a ±1cp oscillation — it is ~1e-6 of it — but the emitted
// integers can, so track the thing that actually ships: the rounded weights.
pub fn rounded(weights: &Weights) -> Vec<i32> {
    weights.iter().flat_map(|w| [w[0].round() as i32, w[1].round() as i32]).collect()
}

pub fn churn(before: &[i32], after: &[i32]) -> usize {
    before.iter().zip(after).filter(|(a, b)| a != b).count()
}

// One walk of the arena. A weight is only as trustworthy as the number of positions
// that pushed on it, and that count lives nowhere else — the gradient sums it away.
pub fn fire_counts(arena: &[u16]) -> Vec<u32> {
    let mut counts = vec![0u32; NUM_PARAMS];
    for packed in arena {
        counts[unpack(*packed).0] += 1;
    }

    counts
}

pub fn write(dir: &str, weights: &Weights, run: &Run) {
    let path = Path::new(dir).join("result.txt");

    std::fs::write(&path, render(weights, run))
        .unwrap_or_else(|e| panic!("cannot write {}: {e}", path.display()));

    println!("wrote {}", path.display());
}

fn render(weights: &Weights, run: &Run) -> String {
    let mut out = String::new();

    writeln!(out, "# Mythos tuning result — {}", timestamp()).unwrap();
    writeln!(out, "# K = {:.6}, {} epochs, train loss {:.6}, validation loss {:.6}",
        run.k, run.epochs, run.last.0, run.last.1).unwrap();
    writeln!(out, "#").unwrap();
    writeln!(out, "# Weights are rounded to i32. \"was\" is the seed value before tuning.").unwrap();
    writeln!(out, "# King safety is not here — it is nonlinear, so the tuner holds it frozen.").unwrap();

    run_section(&mut out, run);
    dataset_section(&mut out, run);
    loss_section(&mut out, run);
    performance_section(&mut out, run);
    diagnostics_section(&mut out, weights, run);
    weights_section(&mut out, weights);

    out
}

fn run_section(out: &mut String, run: &Run) {
    writeln!(out, "\n## Run").unwrap();
    writeln!(out, "  epochs        {}", run.epochs).unwrap();
    writeln!(out, "  K             {:.6}   fitted once at the seed weights, then frozen", run.k).unwrap();
    writeln!(out, "  optimizer     Adam  lr {:.4}  beta1 0.9  beta2 0.999  eps 1e-8", run.lr).unwrap();
    writeln!(out, "  objective     cross-entropy on sigmoid(K · E), E white-relative centipawns").unwrap();
}

fn dataset_section(out: &mut String, run: &Run) {
    let n = run.records as f64;
    let pct = |c: u64| 100.0 * c as f64 / n;

    writeln!(out, "\n## Dataset").unwrap();
    writeln!(out, "  records       {}   {} train / {} validation",
        run.records, run.train_len, run.val_len).unwrap();
    writeln!(out, "  coefficients  {}   mean {:.4} per record",
        run.coeffs, run.coeffs as f64 / n).unwrap();
    writeln!(out, "  labels        {:.1}% 1-0   {:.1}% draw   {:.1}% 0-1",
        pct(run.labels[2]), pct(run.labels[1]), pct(run.labels[0])).unwrap();
    // A prefix/suffix split is only honest because the EPD itself was shuffled on
    // disk. Anyone reading a val loss here should know which of the two it rests on.
    writeln!(out, "  split         first 90% train, last 10% validation — the EPD was shuffled in place,").unwrap();
    writeln!(out, "                so this is a random split and not an ordering artifact").unwrap();
}

fn loss_section(out: &mut String, run: &Run) {
    let below = |loss: f64| 100.0 * (run.baseline - loss) / run.baseline;

    writeln!(out, "\n## Loss").unwrap();
    writeln!(out, "  baseline      {:.6}   flat 0.5, i.e. K = 0 through the same objective (ln 2)",
        run.baseline).unwrap();
    writeln!(out, "  train         {:.6} -> {:.6}   ({:+.6}, {:.2}% below baseline)",
        run.initial.0, run.last.0, run.last.0 - run.initial.0, below(run.last.0)).unwrap();
    writeln!(out, "  validation    {:.6} -> {:.6}   ({:+.6}, {:.2}% below baseline)",
        run.initial.1, run.last.1, run.last.1 - run.initial.1, below(run.last.1)).unwrap();
    // A gap that grows while train keeps falling is the overfitting signature; a flat
    // one says the trace generalises and early stopping would buy nothing.
    writeln!(out, "  train/val gap {:+.6}   grows only if the fit is starting to memorise",
        run.last.1 - run.last.0).unwrap();

    // Epoch 1's train loss is measured *before* that epoch steps, so it must equal
    // fit-k's loss exactly. That equality is the check that gradient() and fit_k
    // share one objective — worth having in the artifact, not just on stdout.
    writeln!(out, "\n  epoch 1 train loss is measured before the first step, so it equals the loss").unwrap();
    writeln!(out, "  `tuner fit-k` reports — the two loss paths agree. churn = rounded i32 weights").unwrap();
    writeln!(out, "  that moved since the row above, counted against the seed weights for epoch 1.").unwrap();
    writeln!(out, "\n  {:>6} {:>12} {:>12} {:>8}", "epoch", "train", "val", "churn").unwrap();
    for row in &run.trace {
        writeln!(out, "  {:>6} {:>12.6} {:>12.6} {:>8}",
            row.epoch, row.train, row.val, row.churn).unwrap();
    }
}

fn performance_section(out: &mut String, run: &Run) {
    let secs = |d: Duration| d.as_secs_f64();
    let total = secs(run.t_open) + secs(run.t_fit_k) + secs(run.t_train) + secs(run.t_diag);

    let per_epoch = if run.epochs > 0 { secs(run.t_train) / run.epochs as f64 } else { 0.0 };

    // Three decimals because open + validate lands in the milliseconds when the
    // mapping is already in page cache, and "0.00 s" would hide a cold-cache run.
    writeln!(out, "\n## Performance").unwrap();
    writeln!(out, "  open + validate   {:>8.3} s   mmap {} records, walk the start chain",
        secs(run.t_open), run.records).unwrap();
    writeln!(out, "  fit K             {:>8.3} s   80 loss passes over precomputed energies",
        secs(run.t_fit_k)).unwrap();
    writeln!(out, "  training          {:>8.3} s   {} epochs, {:.1} ms/epoch (includes the every-10th val pass)",
        secs(run.t_train), run.epochs, per_epoch * 1000.0).unwrap();
    writeln!(out, "  diagnostics       {:>8.3} s   baseline, final gradient, coefficient census",
        secs(run.t_diag)).unwrap();
    writeln!(out, "  total             {:>8.3} s", total).unwrap();

    if per_epoch > 0.0 {
        writeln!(out, "  throughput        {:>8.2} M positions/s   over the {} training records",
            run.train_len as f64 / per_epoch / 1e6, run.train_len).unwrap();
    }
}

fn diagnostics_section(out: &mut String, weights: &Weights, run: &Run) {
    writeln!(out, "\n## Diagnostics").unwrap();

    // A single NaN silently poisons the whole file: it rounds to 0 or i32::MIN and
    // still prints as a number.
    let finite = weights.iter().flatten().filter(|w| w.is_finite()).count();
    writeln!(out, "  finite weights    {finite} / {}   a NaN still rounds to something that prints",
        NUM_PARAMS * 2).unwrap();

    writeln!(out, "  final gradient    max |g| {:.3e} at {} {},  L2 norm {:.3e}",
        run.gradient.max, param_name(run.gradient.at.0),
        if run.gradient.at.1 == 0 { "mg" } else { "eg" }, run.gradient.norm).unwrap();

    // The reproducibility gate. Near a minimum Adam's step is lr·sign(g) whenever
    // |g| >> eps, so weights orbit their optimum at fixed amplitude and the loss
    // cannot tell. If this is not 0, two runs that stop at different epochs emit
    // different integers and no SPRT comparison between them means anything.
    if let Some(row) = run.trace.last() {
        writeln!(out, "  churn at the end  {} of {} rounded weights still moving at epoch {}",
            row.churn, NUM_PARAMS * 2, row.epoch).unwrap();
        if row.churn > 0 {
            writeln!(out, "                    -> not reproducible: decay lr until this reaches 0").unwrap();
        }
    }

    census(out, run);
    movers(out, weights, run);
}

// Arbitrary, but 10k out of 1.4M is where a parameter's fitted value starts being
// set by a handful of games rather than by the data.
const STARVED: u32 = 10_000;

// Adam normalises per parameter, so a feature seen 279 times takes the same size
// step as one seen in every position. The rare buckets are where a tuned table stops
// meaning anything, and the count is the only warning.
fn census(out: &mut String, run: &Run) {
    let never: Vec<usize> = (0..NUM_PARAMS).filter(|&i| run.fired[i] == 0).collect();
    let marked: Vec<usize> = (0..NUM_PARAMS).filter(|&i| is_dead(i)).collect();

    writeln!(out, "\n  parameter census over all {} coefficients in the arena", run.coeffs).unwrap();
    writeln!(out, "    never fired     {} of {NUM_PARAMS}   is_dead marks {}{}",
        never.len(), marked.len(),
        if never.len() == marked.len() && never == marked { " — agreed" } else { "" }).unwrap();

    // A disagreement either way is a bug, not trivia: an unmarked dead parameter is
    // emitted as its stale seed and reads as tuned, and a marked live one is zeroed
    // on the way out, silently deleting a term the eval uses.
    let unmarked: Vec<usize> = never.iter().copied().filter(|&i| !is_dead(i)).collect();
    if !unmarked.is_empty() {
        writeln!(out, "    never fired but not marked dead — emitted as a stale seed:").unwrap();
        for &i in unmarked.iter().take(12) {
            writeln!(out, "      {}", param_name(i)).unwrap();
        }
    }

    let live_marked: Vec<usize> = marked.iter().copied().filter(|&i| run.fired[i] > 0).collect();
    if !live_marked.is_empty() {
        writeln!(out, "    MARKED DEAD BUT FIRES — is_dead is wrong and zeroes a live term:").unwrap();
        for &i in live_marked.iter().take(12) {
            writeln!(out, "      {} fires {} times", param_name(i), run.fired[i]).unwrap();
        }
    }

    let mut live: Vec<usize> = (0..NUM_PARAMS).filter(|&i| run.fired[i] > 0).collect();
    live.sort_by_key(|&i| run.fired[i]);

    let most = live.last().copied().unwrap_or(0);
    let starved = live.iter().filter(|&&i| run.fired[i] < STARVED).count();

    writeln!(out, "    most exercised  {} fires {} times", param_name(most), run.fired[most]).unwrap();
    writeln!(out, "    starved         {starved} live params fire fewer than {STARVED} times, \
        under {:.1}% of the set", 100.0 * STARVED as f64 / run.records as f64).unwrap();
    writeln!(out, "    rarest live     read these as noise, whatever the fitted value says").unwrap();
    for &i in live.iter().take(8) {
        writeln!(out, "      {:<22} {:>9}", param_name(i), run.fired[i]).unwrap();
    }
}

fn movers(out: &mut String, weights: &Weights, run: &Run) {
    let initial = initial_weights();

    let mut moves: Vec<(usize, usize, i32, i32)> = (0..NUM_PARAMS)
        .flat_map(|i| [(i, 0), (i, 1)])
        .filter(|&(i, _)| !is_dead(i))
        .map(|(i, half)| {
            let was = if half == 0 { initial[i].0 } else { initial[i].1 };
            (i, half, was, weights[i][half].round() as i32)
        })
        .collect();
    moves.sort_by_key(|&(_, _, was, now)| -((now - was).abs() as i64));

    writeln!(out, "\n  largest moves from the seed").unwrap();
    for &(i, half, was, now) in moves.iter().take(10) {
        writeln!(out, "    {:<22} {}  {:>6} -> {:>6}   ({:+}, fired {})",
            param_name(i), if half == 0 { "mg" } else { "eg" }, was, now, now - was,
            run.fired[i]).unwrap();
    }
}

// PSQT indices are the position inside the printed 8×8 block below, row-major.
fn param_name(index: usize) -> String {
    let names = ["Pawn", "Knight", "Bishop", "Rook", "Queen", "King"];

    match index {
        i if i < TEMPO => format!("PSQT {}[{}]", names[i / 64], i % 64),
        TEMPO => "TEMPO".to_string(),
        BISHOP_PAIR => "BISHOP_PAIR".to_string(),
        ISOLATED_PAWN => "ISOLATED_PAWN".to_string(),
        DOUBLED_PAWN => "DOUBLED_PAWN".to_string(),
        i if i < ISOLATED_PAWN => format!("PASSED_PAWN[{}]", i - PASSED_PAWN),
        i if i < BISHOP_MOB => format!("KNIGHT_MOB[{}]", i - KNIGHT_MOB),
        i if i < ROOK_MOB => format!("BISHOP_MOB[{}]", i - BISHOP_MOB),
        i if i < QUEEN_MOB => format!("ROOK_MOB[{}]", i - ROOK_MOB),
        i if i < NUM_PARAMS => format!("QUEEN_MOB[{}]", i - QUEEN_MOB),
        i => format!("param {i} (out of range)")
    }
}

// No chrono for one line of output. Howard Hinnant's civil_from_days, which is exact
// for any date after 1970 — the run's date is what tells two result.txt apart.
fn timestamp() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs()) as i64;
    let (days, rem) = (secs.div_euclid(86400), secs.rem_euclid(86400));

    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;

    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };

    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02} UTC", rem / 3600, (rem % 3600) / 60, rem % 60)
}

// Coefficient is always zero for these, so no position ever produces a gradient for
// them and Adam leaves them at whatever they were seeded with. Report zero rather
// than a stale seed that reads as a tuned value.
fn is_dead(index: usize) -> bool {
    let pawn_square = index.wrapping_sub(PSQT);

    (pawn_square < 8 || (56..64).contains(&pawn_square))
        || index == PASSED_PAWN
        || index == PASSED_PAWN + 7
}

fn weights_section(out: &mut String, weights: &Weights) {
    let initial = initial_weights();

    let now = |i: usize, half: usize| -> i32 {
        if is_dead(i) { 0 } else { weights[i][half].round() as i32 }
    };
    let was = |i: usize, half: usize| -> i32 {
        if is_dead(i) { return 0 }
        if half == 0 { initial[i].0 } else { initial[i].1 }
    };

    let names = ["Pawn", "Knight", "Bishop", "Rook", "Queen", "King"];

    writeln!(out, "\n## Implied piece values (mean of each PSQT block)").unwrap();
    for (pt, name) in names.iter().enumerate() {
        let squares: Vec<usize> = if pt == 0 { (8..56).collect() } else { (0..64).collect() };
        let mean = |half: usize| -> f64 {
            squares.iter().map(|&sq| now(PSQT + pt * 64 + sq, half) as f64).sum::<f64>()
                / squares.len() as f64
        };
        let mean_was = |half: usize| -> f64 {
            squares.iter().map(|&sq| was(PSQT + pt * 64 + sq, half) as f64).sum::<f64>()
                / squares.len() as f64
        };

        writeln!(out, "  {name:<7} mg {:>7.1} (was {:>6.1})   eg {:>7.1} (was {:>6.1})",
            mean(0), mean_was(0), mean(1), mean_was(1)).unwrap();
    }

    writeln!(out, "\n## PSQT").unwrap();
    for (pt, name) in names.iter().enumerate() {
        for (half, tag) in ["mg", "eg"].iter().enumerate() {
            writeln!(out, "\n### {name} {tag}").unwrap();
            for rank in 0..8 {
                for file in 0..8 {
                    write!(out, "{:5},", now(PSQT + pt * 64 + rank * 8 + file, half)).unwrap();
                }
                out.push('\n');
            }
        }
    }

    writeln!(out, "\n## Scalars").unwrap();
    for (label, index) in [
        ("TEMPO", TEMPO),
        ("BISHOP_PAIR", BISHOP_PAIR),
        ("ISOLATED_PAWN", ISOLATED_PAWN),
        ("DOUBLED_PAWN", DOUBLED_PAWN),
    ] {
        writeln!(out, "  {label:<16} mg {:>6} (was {:>5})   eg {:>6} (was {:>5})",
            now(index, 0), was(index, 0), now(index, 1), was(index, 1)).unwrap();
    }


    writeln!(out, "\n## Tables").unwrap();
    for (label, base, len) in [
        ("PASSED_PAWN",    PASSED_PAWN, 8),
        ("KNIGHT_MOBILITY", KNIGHT_MOB, 9),
        ("BISHOP_MOBILITY", BISHOP_MOB, 14),
        ("ROOK_MOBILITY",     ROOK_MOB, 15),
        ("QUEEN_MOBILITY",   QUEEN_MOB, 28),
    ] {
        writeln!(out, "\n### {label}").unwrap();
        for n in 0..len {
            writeln!(out, "  [{n:>2}]  mg {:>6} (was {:>5})   eg {:>6} (was {:>5})",
                now(base + n, 0), was(base + n, 0), now(base + n, 1), was(base + n, 1)).unwrap();
        }
    }
}
