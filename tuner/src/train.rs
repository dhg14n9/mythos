use std::ops::Range;
use std::time::Instant;
use rayon::prelude::*;
use mythos::eval::trace::{initial_weights, NUM_PARAMS, TEMPO};
use crate::dataset::Dataset;
use crate::format::{unpack, Record};
use crate::{emit, report};

pub type Weights = [[f64; 2]; NUM_PARAMS];

const ZERO: Weights = [[0f64; 2]; NUM_PARAMS];

const CHUNK: usize = 16_384;

fn sigmoid_and_log(z: f64) -> (f64, f64) {
    if z >= 0.0 {
        let e = (-z).exp();     // (0, 1]
        (1.0 / (1.0 + e), -e.ln_1p())
    } else {
        let e = z.exp();        // (0, 1)
        (e / (1.0 + e), z - e.ln_1p())
    }
}

fn from_initial() -> Weights {
    let mut result: Weights = [[0f64; 2]; NUM_PARAMS];
    let initial_weights = initial_weights();
    for (i, s) in initial_weights.iter().enumerate() {
        result[i][0] = s.0 as f64;
        result[i][1] = s.1 as f64;
    }

    result
}

fn accumulate(records: &[Record], arena: &[u16], weights: &Weights, k: f64) -> (Weights, f64) {
    let taper: [[f64; 2]; 25] = std::array::from_fn(|phase| {
        let mg = phase as f64 / 24.0;
        [mg, 1.0 - mg]
    });

    let mut gradient = ZERO;
    let mut ll = 0f64;

    for record in records {
        let start = record.start as usize;
        let coeffs = &arena[start..start + record.len as usize];

        let [mg_weight, eg_weight] = taper[record.phase as usize];

        let mut mg = 0f64;
        let mut eg = 0f64;
        for packed in coeffs {
            let (index, value) = unpack(*packed);
            mg += weights[index][0] * value as f64;
            eg += weights[index][1] * value as f64;
        }

        let z = k * (mg * mg_weight + eg * eg_weight + record.frozen as f64);
        let (s, log_s) = sigmoid_and_log(z);
        let r = record.get_result();

        ll += log_s - (1.0 - r) * z;

        let g = k * (s - r);

        for packed in coeffs {
            let (index, value) = unpack(*packed);
            gradient[index][0] += g * mg_weight * value as f64;
            gradient[index][1] += g * eg_weight * value as f64;
        }
    }

    (gradient, ll)
}

fn pass(records: &[Record], arena: &[u16], weights: &Weights, k: f64) -> (Weights, f64) {
    records.par_chunks(CHUNK)
        .map(|chunk| accumulate(chunk, arena, weights, k))
        .reduce(|| (ZERO, 0f64), |mut a, b| {
            for i in 0..NUM_PARAMS {
                a.0[i][0] += b.0[i][0];
                a.0[i][1] += b.0[i][1];
            }
            a.1 += b.1;

            a
        })
}

pub struct Trainer {
    dataset: Dataset,
    weights: Weights,
    k: f64,
    split: usize,
    adam: Adam,
    schedule: Schedule
}

impl Trainer {
    pub fn new(dataset: Dataset) -> Self {
        let split = dataset.len() * 9 / 10;
        let weights = from_initial();
        let mut trainer = Self {
            dataset, schedule: Schedule::new(&weights), weights, k: 0.0, split,
            adam: Adam::new(1.0)
        };
        trainer.k = trainer.fit_k();

        trainer
    }

    pub fn k(&self) -> f64 {
        self.k
    }

    pub fn lr(&self) -> f64 {
        self.adam.lr
    }

    pub fn weights(&self) -> &Weights {
        &self.weights
    }

    pub fn dataset(&self) -> &Dataset {
        &self.dataset
    }

    pub fn baseline(&self) -> f64 {
        Self::loss(self.train_records(), self.dataset.arena(), &self.weights, 0.0)
    }

    pub fn train_range(&self) -> Range<usize> {
        0..self.split
    }

    pub fn val_range(&self) -> Range<usize> {
        self.split..self.dataset.len()
    }

    fn train_records(&self) -> &[Record] {
        &self.dataset.records()[self.train_range()]
    }

    fn val_records(&self) -> &[Record] {
        &self.dataset.records()[self.val_range()]
    }

    fn energy(record: &Record, coeffs: &[u16], weights: &Weights) -> f64 {
        let mut mg = 0f64;
        let mut eg = 0f64;

        let mg_weight = record.phase as f64 / 24.0;
        let eg_weight = 1.0 - mg_weight;
        for packed in coeffs {
            let (i, value) = unpack(*packed);
            mg += value as f64 * weights[i][0];
            eg += value as f64 * weights[i][1];
        }

        mg * mg_weight + eg * eg_weight + record.frozen as f64
    }

    fn loss(records: &[Record], arena: &[u16], weights: &Weights, k: f64) -> f64 {
        let (_, ll) = pass(records, arena, weights, k);

        -ll / records.len() as f64
    }

    pub fn frozen_energies(&self, range: Range<usize>) -> (Vec<f64>, Vec<f64>) {
        let arena = self.dataset.arena();
        let records = &self.dataset.records()[range];

        let energies = records.par_iter()
            .map(|record| {
                let start = record.start as usize;
                let coeffs = &arena[start..start + record.len as usize];

                Self::energy(record, coeffs, &self.weights)
            })
            .collect();

        let results = records.iter().map(Record::get_result).collect();

        (energies, results)
    }

    pub fn fit_k(&self) -> f64 {
        let (energies, results) = self.frozen_energies(self.train_range());

        let mut lo = 0.001f64;
        let mut hi = 0.02f64;
        for _ in 0..40 {
            let m1 = lo + (hi - lo) / 3.0;
            let m2 = hi - (hi - lo) / 3.0;
            if loss_over(&energies, &results, m1) < loss_over(&energies, &results, m2) {
                hi = m2
            } else {
                lo = m1
            }
        }

        (lo + hi) / 2.0
    }

    pub fn epoch(&mut self) -> f64 {
        let (gradient, loss) = self.gradient();
        self.adam.step(&mut self.weights, &gradient);

        loss
    }

    pub fn val_loss(&self) -> f64 {
        Self::loss(self.val_records(), self.dataset.arena(), &self.weights, self.k)
    }

    pub fn observe(&mut self, val: f64) -> bool {
        if val < self.schedule.best - Schedule::IMPROVE {
            self.schedule.best = val;
            self.schedule.stale = 0;
        } else {
            self.schedule.stale += 1;

            if self.schedule.stale >= Schedule::PATIENCE {
                self.adam.lr *= Schedule::DECAY;
                self.schedule.stale = 0;
            }
        }

        let now = report::rounded(&self.weights);
        if report::churn(&self.schedule.rounded, &now) == 0 {
            self.schedule.stable += 1;
        } else {
            self.schedule.stable = 0;
        }
        self.schedule.rounded = now;

        self.adam.lr < Schedule::MIN_LR && self.schedule.stable >= Schedule::PATIENCE
    }

    fn gradient(&self) -> (Weights, f64) {
        let records = self.train_records();
        let len = records.len() as f64;

        let (mut gradient, ll) = pass(records, self.dataset.arena(), &self.weights, self.k);
        for g in gradient.iter_mut() {
            g[0] /= len;
            g[1] /= len;
        }

        (gradient, -ll / len)
    }
}

struct Schedule {
    best: f64,
    stale: usize,
    rounded: Vec<i32>,
    stable: usize
}

impl Schedule {
    const IMPROVE: f64 = 1e-6;
    const PATIENCE: usize = 10;
    const DECAY: f64 = 0.9;
    const MIN_LR: f64 = 0.01;

    fn new(weights: &Weights) -> Self {
        Self { best: f64::INFINITY, stale: 0, rounded: report::rounded(weights), stable: 0 }
    }
}

pub struct Adam {
    m: Weights,
    v: Weights,
    t: u64,
    lr: f64
}


impl Adam {
    const BETA1: f64 = 0.9;
    const BETA2: f64 = 0.999;
    const EPS: f64 = 1e-8;

    fn new(lr: f64) -> Self {
        Self {
            m: [[0.0; 2]; NUM_PARAMS],
            v: [[0.0; 2]; NUM_PARAMS],
            t: 0,
            lr
        }
    }

    fn step(&mut self, weights: &mut Weights, grads: &Weights) {
        self.t += 1;

        let bc1 = 1.0 - Self::BETA1.powi(self.t as i32);
        let bc2 = 1.0 - Self::BETA2.powi(self.t as i32);

        let len = weights.len();
        for i in 0..len {
            let g = grads[i];


            self.m[i][0] = Self::BETA1 * self.m[i][0] + (1.0 - Self::BETA1) * g[0];
            self.m[i][1] = Self::BETA1 * self.m[i][1] + (1.0 - Self::BETA1) * g[1];
            self.v[i][0] = Self::BETA2 * self.v[i][0] + (1.0 - Self::BETA2) * g[0] * g[0];
            self.v[i][1] = Self::BETA2 * self.v[i][1] + (1.0 - Self::BETA2) * g[1] * g[1];

            let m_hat_0 = self.m[i][0] / bc1;
            let m_hat_1 = self.m[i][1] / bc1;
            let v_hat_0 = self.v[i][0] / bc2;
            let v_hat_1 = self.v[i][1] / bc2;

            weights[i][0] -= self.lr * m_hat_0 / (v_hat_0.sqrt() + Self::EPS);
            weights[i][1] -= self.lr * m_hat_1 / (v_hat_1.sqrt() + Self::EPS);
        }

    }

}


fn loss_over(energies: &[f64], results: &[f64], k: f64) -> f64 {
    let partial: Vec<f64> = energies.par_chunks(CHUNK)
        .zip(results.par_chunks(CHUNK))
        .map(|(energies, results)| {
            let mut sum = 0f64;
            for i in 0..energies.len() {
                // ln(s) − (1−r)·z, the same rewrite as `accumulate`. This is where
                // the whole startup cost lives: 80 passes for the ternary search.
                let z = k * energies[i];
                sum += sigmoid_and_log(z).1 - (1.0 - results[i]) * z;
            }

            sum
        })
        .collect();

    -partial.iter().sum::<f64>() / (energies.len() as f64)
}

pub fn fit_k(dir: &str) {
    let trainer = Trainer::new(Dataset::open(dir));
    let (energies, results) = trainer.frozen_energies(trainer.train_range());

    println!("loss curve:");
    for i in 0..=20 {
        let k = 0.001 + (0.02 - 0.001) * i as f64 / 20.0;
        println!("  K = {k:.5}  loss = {:.6}", loss_over(&energies, &results, k));
    }

    // Trainer::new already fit this — re-running the search here would be a third
    // pass over the data for an answer we hold.
    let k = trainer.k();
    let loss = loss_over(&energies, &results, k);

    // sigmoid(0) is exactly 0.5, so K = 0 evaluates the same loss at a flat
    // prediction — the baseline is the objective itself, not a second formula
    // that has to be kept in sync with it. Equals ln 2 for cross-entropy.
    let baseline = loss_over(&energies, &results, 0.0);

    println!();
    println!("K        = {k:.6}");
    println!("loss     = {loss:.6} (cross-entropy)");
    println!("baseline = {baseline:.6} (flat 0.5)");

    assert!(loss < loss_over(&energies, &results, 0.001)
         && loss < loss_over(&energies, &results, 0.02),
        "fitted K is worse than a bracket endpoint — the search ran backwards");
}

// gradient() is the chain rule done by hand, which is where sign flips and dropped
// constants hide. But a derivative is only a slope, and a slope can be measured
// without any calculus: nudge one weight, see how far the loss moved. If the two
// disagree, the symbolic version is wrong.
//
// Worth the runtime because every plausible mistake here is invisible downstream —
// a stray factor, a missing 1/N, even a dropped s(1-s) all still point roughly
// downhill, so the loss falls and the run looks healthy while fitting the wrong
// function. This is the §4 counterpart to the trace equivalence test.
//
// Since 1a the loss it probes *is* the gradient's own arithmetic, so this no longer
// catches a disagreement between two copies of the formula — there is only one. What
// it still catches is the calculus: the analytic scatter against the slope the
// objective actually has.
pub fn check_gradient(dir: &str) {
    let mut trainer = Trainer::new(Dataset::open(dir));
    let (analytic, _) = trainer.gradient();
    let range = trainer.train_range();
    let k = trainer.k();

    // Weights are centipawn-scale, so h this large is still a small relative nudge.
    // The textbook 1e-5 would put the loss difference down among the f64 rounding
    // noise of summing 1.28M terms.
    let h = 1e-2;

    // Structurally dead params have a true gradient of exactly zero and would match
    // trivially, proving nothing. Ranking by magnitude keeps the probes on params
    // the data actually exercises. TEMPO is pinned because every position touches it.
    let mut ranked: Vec<(usize, usize)> =
        (0..NUM_PARAMS).flat_map(|i| [(i, 0), (i, 1)]).collect();
    ranked.sort_by(|a, b| analytic[b.0][b.1].abs().total_cmp(&analytic[a.0][a.1].abs()));

    let mut probes = vec![(TEMPO, 0), (TEMPO, 1)];
    for &p in ranked.iter().take(10) {
        if !probes.contains(&p) {
            probes.push(p);
        }
    }

    println!("central differences at h = {h}, over {} training positions", range.len());
    println!("{:>5} {:>5} {:>16} {:>16} {:>10}", "param", "half", "analytic", "numeric", "rel err");

    let mut worst = 0f64;
    for (i, j) in probes {
        let original = trainer.weights[i][j];

        trainer.weights[i][j] = original + h;
        let up = Trainer::loss(trainer.train_records(), trainer.dataset.arena(), &trainer.weights, k);

        trainer.weights[i][j] = original - h;
        let down = Trainer::loss(trainer.train_records(), trainer.dataset.arena(), &trainer.weights, k);

        // Restore before the next probe, or the perturbations compound and every
        // measurement after the first is taken at the wrong point.
        trainer.weights[i][j] = original;

        // Central, not one-sided: the error is O(h²) instead of O(h), which is the
        // difference between matching to six digits and matching to two.
        let numeric = (up - down) / (2.0 * h);
        let a = analytic[i][j];

        // Relative, because gradient magnitudes span orders of magnitude across
        // params — one absolute tolerance would either pass everything or fail it.
        let rel = (a - numeric).abs() / a.abs().max(numeric.abs()).max(1e-12);
        worst = worst.max(rel);

        println!("{i:>5} {:>5} {a:>16.9} {numeric:>16.9} {rel:>10.2e}",
            if j == 0 { "mg" } else { "eg" });
    }

    println!();
    println!("worst relative error = {worst:.2e}");

    assert!(worst < 1e-4,
        "gradient disagrees with finite differences. A clean ratio between the two \
         columns names the dropped factor: 2 = differentiated squared error instead \
         of cross-entropy, N = missing the 1/N, 24 = a taper weight applied to E but \
         not to the gradient. A sign flip is r-s vs s-r.");
}

pub fn train(dir: &str, epochs: usize) {
    let mark = Instant::now();
    let dataset = Dataset::open(dir);
    let t_open = mark.elapsed();

    let mark = Instant::now();
    let mut trainer = Trainer::new(dataset);
    let t_fit_k = mark.elapsed();

    let initial_lr = trainer.lr();

    println!("K = {:.6} (frozen for the whole run)", trainer.k());
    println!("{} training / {} validation positions",
        trainer.train_range().len(), trainer.val_range().len());
    println!("{} rayon threads, {CHUNK} records per chunk", rayon::current_num_threads());
    println!("--epochs {epochs} is a safety cap; the run ends when the emitted weights settle");
    println!();
    println!("{:>6} {:>12} {:>12} {:>8} {:>8}", "epoch", "train", "val", "lr", "churn");

    let initial_val = trainer.val_loss();
    let mut rounded = report::rounded(trainer.weights());

    let mut trace: Vec<report::Epoch> = Vec::new();
    let mut train = f64::NAN;
    let mut val = initial_val;
    let mut ran = 0usize;
    let mut stop = "hit the --epochs cap — raise it or lower PATIENCE";

    let mark = Instant::now();
    for epoch in 1..=epochs {
        train = trainer.epoch();
        val = trainer.val_loss();
        ran = epoch;

        let settled = trainer.observe(val);
        if settled {
            stop = "converged — lr below the floor and no emitted weight moved for 10 epochs";
        }

        // Print every tenth, but never miss the row the reproducibility gate reads.
        if epoch == 1 || epoch % 10 == 0 || settled || epoch == epochs {
            let now = report::rounded(trainer.weights());
            let churn = report::churn(&rounded, &now);
            rounded = now;

            let lr = trainer.lr();
            println!("{epoch:>6} {train:>12.6} {val:>12.6} {lr:>8.4} {churn:>8}");
            trace.push(report::Epoch { epoch, train, val, lr, churn });
        }

        if settled { break }
    }
    let t_train = mark.elapsed();

    println!("\nstopped after {ran} epochs: {stop}");

    let mark = Instant::now();
    let baseline = trainer.baseline();
    let (gradient, _) = trainer.gradient();
    let fired = report::fire_counts(trainer.dataset().arena());
    let t_diag = mark.elapsed();

    let run = report::Run {
        epochs: ran,
        cap: epochs,
        stop,
        k: trainer.k(),
        lr: (initial_lr, trainer.lr()),
        threads: rayon::current_num_threads(),

        records: trainer.dataset().len(),
        coeffs: trainer.dataset().arena().len(),
        labels: trainer.dataset().label_counts(),
        train_len: trainer.train_range().len(),
        val_len: trainer.val_range().len(),

        baseline,
        initial: (trace.first().map_or(f64::NAN, |row| row.train), initial_val),
        last: (train, val),
        trace,

        t_open, t_fit_k, t_train, t_diag,

        gradient: report::GradStats::of(&gradient),
        fired
    };

    println!();
    report::write(dir, trainer.weights(), &run);
    emit::save(dir, trainer.weights(), &run);
}
