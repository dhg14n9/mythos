//! Post-run analysis for SPRT matches.
//!
//! fastchess prints LLR/Elo/nElo only to the terminal; the `config.json` it
//! writes into each run directory has the raw pentanomial tallies but none of
//! the derived statistics. This module recomputes them from `config.json` and
//! renders `report.md` — a self-contained account of the run that also
//! explains every concept and value to a reader new to engine testing.
//!
//! The math is a faithful port of fastchess's `model=normalized` pentanomial
//! implementation (`sprt.cpp`, `elo_pentanomial.cpp`), which follows Michel
//! Van den Bergh's write-ups (cantate.be/Fishtest). The closed-form LLR
//! approximation is deliberately NOT used: it disagrees with the exact GSPRT
//! MLE by ~25% on real data, so the full MLE + ITP root finder is ported.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

use crate::util::{Result, run_capture, workspace_root};

const REPORT_FILE: &str = "report.md";

/// The normalized-Elo scale factor 800/ln(10).
const NELO_SCALE: f64 = 800.0 / std::f64::consts::LN_10;
/// Two-sided 95% normal quantile (fastchess's CI95ZSCORE).
const Z95: f64 = 1.959_963_984_540_054;
/// Pair-score support, in category order [LL, LD, WL+DD, WD, WW]:
/// a pair is two games, scored 0, 0.5, 1, 1.5 or 2 points, divided by 2.
const PAIR_SCORES: [f64; 5] = [0.0, 0.25, 0.5, 0.75, 1.0];

#[derive(Clone, Copy, Debug)]
struct Penta {
    ww: u64,
    wd: u64,
    wl: u64,
    dd: u64,
    ld: u64,
    ll: u64,
}

impl Penta {
    fn pairs(&self) -> u64 {
        self.ww + self.wd + self.wl + self.dd + self.ld + self.ll
    }
}

/// Everything the report needs, all parsed from `config.json`.
#[derive(Debug)]
struct RunData {
    run_name: String,
    pair_name: String,
    base_sha: String,
    wins: u64,
    losses: u64,
    draws: u64,
    penta: Penta,
    elo0: f64,
    elo1: f64,
    alpha: f64,
    beta: f64,
    model: String,
    rounds_cap: u64,
    tc: String,
    book: String,
    adjudication: String,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Verdict {
    AcceptH1,
    AcceptH0,
    Inconclusive,
}

#[derive(Debug)]
struct Analysis {
    pairs: u64,
    games: u64,
    points: f64,
    score_pct: f64,
    // Display uses score_pct; tests assert this directly as the input that
    // var/Elo/nElo are derived from.
    #[allow(dead_code)]
    pair_score: f64,
    pair_var: f64,
    llr: f64,
    lower: f64,
    upper: f64,
    elo: f64,
    elo_err: f64,
    nelo: f64,
    nelo_err: f64,
    los: f64,
    draw_pct_games: f64,
    draw_pct_pairs: f64,
    pairs_ratio: f64,
    wl_dd: f64,
    verdict: Verdict,
}

/// Generate `report.md` for a run directory containing a fastchess
/// `config.json`; prints a short terminal summary and returns the report path.
pub fn generate(run_dir: &Path) -> Result<PathBuf> {
    let config_path = run_dir.join("config.json");
    let text = std::fs::read_to_string(&config_path)
        .map_err(|e| format!("cannot read {}: {e}", config_path.display()))?;
    let run_name = run_dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "run".into());

    let data = parse_config(&text, &run_name)?;
    let analysis = analyze(&data)?;

    let generated =
        run_capture(Command::new("date").arg("+%Y-%m-%d %H:%M")).unwrap_or_else(|_| "?".into());
    let report = render(&data, &analysis, &generated);

    let path = run_dir.join(REPORT_FILE);
    std::fs::write(&path, report).map_err(|e| format!("cannot write {}: {e}", path.display()))?;

    print_summary(&data, &analysis);
    Ok(path)
}

/// `cargo xtask sprt-report <run>` — accepts a path or a bare run name.
pub fn report_cmd(arg: &str) -> Result<()> {
    let root = workspace_root();
    let candidates = [
        PathBuf::from(arg),
        root.join(arg),
        root.join("target/sprt/runs").join(arg),
    ];
    let run_dir = candidates.iter().find(|p| p.is_dir()).ok_or_else(|| {
        let tried: Vec<String> = candidates.iter().map(|p| p.display().to_string()).collect();
        format!("no such run directory: {arg}\ntried:\n  {}", tried.join("\n  "))
    })?;
    let path = generate(run_dir)?;
    println!("[sprt] report: {}", path.display());
    Ok(())
}

// ---------------------------------------------------------------------------
// config.json parsing

fn parse_config(text: &str, run_name: &str) -> Result<RunData> {
    let v: Value = serde_json::from_str(text).map_err(|e| format!("config.json: {e}"))?;

    let sprt = v
        .get("sprt")
        .filter(|s| s.is_object())
        .ok_or("config.json: missing sprt section")?;
    let model = sprt
        .get("model")
        .and_then(Value::as_str)
        .ok_or("config.json: missing sprt.model")?
        .to_string();

    let stats = v
        .get("stats")
        .and_then(Value::as_object)
        .filter(|s| !s.is_empty())
        .ok_or("config.json has no game statistics — fastchess exited before any pair finished")?;
    let (pair_name, st) = match stats.get_key_value("dev vs base") {
        Some((k, s)) => (k.clone(), s),
        None => {
            let (k, s) = stats.iter().next().expect("stats checked non-empty");
            (k.clone(), s)
        }
    };

    Ok(RunData {
        run_name: run_name.to_string(),
        base_sha: base_sha(&v, run_name),
        wins: num_u64(st, "wins", &pair_name)?,
        losses: num_u64(st, "losses", &pair_name)?,
        draws: num_u64(st, "draws", &pair_name)?,
        penta: Penta {
            ww: num_u64(st, "penta_WW", &pair_name)?,
            wd: num_u64(st, "penta_WD", &pair_name)?,
            wl: num_u64(st, "penta_WL", &pair_name)?,
            dd: num_u64(st, "penta_DD", &pair_name)?,
            ld: num_u64(st, "penta_LD", &pair_name)?,
            ll: num_u64(st, "penta_LL", &pair_name)?,
        },
        pair_name,
        elo0: num_f64(sprt, "elo0", "sprt")?,
        elo1: num_f64(sprt, "elo1", "sprt")?,
        alpha: num_f64(sprt, "alpha", "sprt")?,
        beta: num_f64(sprt, "beta", "sprt")?,
        model,
        // The rest is cosmetic context; missing fields degrade, never fail.
        rounds_cap: v.get("rounds").and_then(Value::as_u64).unwrap_or(0),
        tc: fmt_tc(&v).unwrap_or_else(|| "?".into()),
        book: v
            .get("opening")
            .and_then(|o| o.get("file"))
            .and_then(Value::as_str)
            .map(|f| {
                Path::new(f)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| f.to_string())
            })
            .unwrap_or_else(|| "?".into()),
        adjudication: adjudication_summary(&v),
    })
}

fn num_f64(v: &Value, key: &str, ctx: &str) -> Result<f64> {
    v.get(key)
        .and_then(Value::as_f64)
        .ok_or_else(|| format!("config.json: missing or non-numeric {ctx}.{key}"))
}

fn num_u64(v: &Value, key: &str, ctx: &str) -> Result<u64> {
    v.get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("config.json: missing or non-numeric {ctx}.{key}"))
}

/// The baseline sha, from the base engine's binary name `mythos-base-<sha>`,
/// falling back to the `<stamp>-vs-<sha>` run-directory name.
fn base_sha(v: &Value, run_name: &str) -> String {
    v.get("engines")
        .and_then(Value::as_array)
        .and_then(|engines| {
            engines
                .iter()
                .find(|e| e.get("name").and_then(Value::as_str) == Some("base"))
                .and_then(|e| e.get("cmd"))
                .and_then(Value::as_str)
                .and_then(|cmd| Path::new(cmd).file_name())
                .and_then(|n| n.to_str())
                .and_then(|n| n.strip_prefix("mythos-base-"))
                .map(str::to_string)
        })
        .or_else(|| run_name.split_once("-vs-").map(|(_, sha)| sha.to_string()))
        .unwrap_or_else(|| "?".into())
}

/// Reconstruct the `8+0.08` time-control string from the first engine's
/// limits (both engines get the same `-each tc=`).
fn fmt_tc(v: &Value) -> Option<String> {
    let tc = v.get("engines")?.get(0)?.get("limit")?.get("tc")?;
    let ms = |key: &str| tc.get(key).and_then(Value::as_f64);
    let secs = |x: f64| format!("{}", x / 1000.0);
    if ms("fixed_time").unwrap_or(0.0) > 0.0 {
        return Some(format!("{} s per move", secs(ms("fixed_time")?)));
    }
    Some(format!("{}+{}", secs(ms("time")?), secs(ms("increment").unwrap_or(0.0))))
}

fn adjudication_summary(v: &Value) -> String {
    let enabled = |section: &Value| section.get("enabled").and_then(Value::as_bool) == Some(true);
    let num = |section: &Value, key: &str| section.get(key).and_then(Value::as_u64).unwrap_or(0);

    // No '|' in this text: it is embedded in a Markdown table cell.
    let mut parts = Vec::new();
    if let Some(d) = v.get("draw").filter(|d| enabled(d)) {
        parts.push(format!(
            "draw if eval stays within +/-{} cp for {} moves from move {}",
            num(d, "score"),
            num(d, "move_count"),
            num(d, "move_number"),
        ));
    }
    if let Some(r) = v.get("resign").filter(|r| enabled(r)) {
        let twosided = r.get("twosided").and_then(Value::as_bool) == Some(true);
        parts.push(format!(
            "resign if eval reaches +/-{} cp for {} moves{}",
            num(r, "score"),
            num(r, "move_count"),
            if twosided { " (both engines agree)" } else { "" },
        ));
    }
    if let Some(m) = v.get("maxmoves").filter(|m| enabled(m)) {
        parts.push(format!("draw at move {}", num(m, "move_count")));
    }
    if parts.is_empty() { "none".into() } else { parts.join("; ") }
}

// ---------------------------------------------------------------------------
// statistics (ported from fastchess: elo_pentanomial.cpp, sprt.cpp)

fn analyze(d: &RunData) -> Result<Analysis> {
    if d.model != "normalized" {
        return Err(format!(
            "unsupported SPRT model '{}' in config.json — this runner always uses model=normalized",
            d.model
        ));
    }
    let pairs = d.penta.pairs();
    if pairs == 0 {
        return Err("no completed game pairs in config.json — fastchess exited before any pair finished".into());
    }

    let p = &d.penta;
    let n = pairs as f64;
    let frac = |c: u64| c as f64 / n;

    let score =
        (p.ww as f64 + 0.75 * p.wd as f64 + 0.5 * (p.wl + p.dd) as f64 + 0.25 * p.ld as f64) / n;
    let var = frac(p.ww) * (1.0 - score).powi(2)
        + frac(p.wd) * (0.75 - score).powi(2)
        + (frac(p.wl) + frac(p.dd)) * (0.5 - score).powi(2)
        + frac(p.ld) * (0.25 - score).powi(2)
        + frac(p.ll) * (0.0 - score).powi(2);
    let var_mean = var / n;
    let s_hi = score + Z95 * var_mean.sqrt();
    let s_lo = score - Z95 * var_mean.sqrt();

    let llr = llr_penta(*p, d.elo0, d.elo1);
    let lower = (d.beta / (1.0 - d.alpha)).ln();
    let upper = ((1.0 - d.beta) / d.alpha).ln();
    let verdict = if llr >= upper {
        Verdict::AcceptH1
    } else if llr <= lower {
        Verdict::AcceptH0
    } else {
        Verdict::Inconclusive
    };

    let games = d.wins + d.draws + d.losses;
    let points = d.wins as f64 + 0.5 * d.draws as f64;

    Ok(Analysis {
        pairs,
        games,
        points,
        score_pct: 100.0 * points / games as f64,
        pair_score: score,
        pair_var: var,
        llr,
        lower,
        upper,
        elo: score_to_elo(score),
        elo_err: (score_to_elo(s_hi) - score_to_elo(s_lo)) / 2.0,
        nelo: score_to_nelo(score, var),
        nelo_err: (score_to_nelo(s_hi, var) - score_to_nelo(s_lo, var)) / 2.0,
        los: (1.0 - erf(-(score - 0.5) / (2.0 * var_mean).sqrt())) / 2.0,
        draw_pct_games: 100.0 * d.draws as f64 / games as f64,
        draw_pct_pairs: 100.0 * (p.wl + p.dd) as f64 / n,
        pairs_ratio: (p.ww + p.wd) as f64 / (p.ld + p.ll) as f64,
        wl_dd: p.wl as f64 / p.dd as f64,
        verdict,
    })
}

/// Logistic Elo difference for a score in (0, 1).
fn score_to_elo(score: f64) -> f64 {
    -400.0 * (1.0 / score - 1.0).log10()
}

/// Normalized Elo difference: score offset in units of the per-game
/// standard deviation (pair variance × 2), on the 800/ln(10) scale.
fn score_to_nelo(score: f64, var: f64) -> f64 {
    (score - 0.5) / (2.0 * var).sqrt() * NELO_SCALE
}

/// GSPRT log-likelihood ratio for pentanomial counts under the normalized
/// model (elo0/elo1 are normalized-Elo bounds). Port of `SPRT::getLLR`.
fn llr_penta(p: Penta, elo0: f64, elo1: f64) -> f64 {
    // fastchess "regularizes" empty categories so no probability is zero.
    let regularize = |n: u64| if n == 0 { 1e-3 } else { n as f64 };
    let ll = regularize(p.ll);
    let ld = regularize(p.ld);
    let wl_dd = regularize(p.dd + p.wl);
    let wd = regularize(p.wd);
    let ww = regularize(p.ww);
    let total = ww + wd + wl_dd + ld + ll;
    let probs = [ll / total, ld / total, wl_dd / total, wd / total, ww / total];

    // √2: the hypothesized t-value is per game; pair scores have √2× the
    // normalized offset (var(pair mean) ≈ var(game)/2).
    let t0 = std::f64::consts::SQRT_2 * elo0 / NELO_SCALE;
    let t1 = std::f64::consts::SQRT_2 * elo1 / NELO_SCALE;

    let p0 = mle(&probs, 0.5, t0);
    let p1 = mle(&probs, 0.5, t1);
    let lpr: [f64; 5] = std::array::from_fn(|i| p1[i].ln() - p0[i].ln());
    total * mean(&lpr, &probs)
}

fn mean(x: &[f64; 5], p: &[f64; 5]) -> f64 {
    (0..5).map(|i| x[i] * p[i]).sum()
}

fn mean_and_variance(x: &[f64; 5], p: &[f64; 5]) -> (f64, f64) {
    let mu = mean(x, p);
    let var = (0..5).map(|i| p[i] * (x[i] - mu) * (x[i] - mu)).sum();
    (mu, var)
}

/// Maximum-likelihood pentanomial distribution subject to a fixed normalized
/// score t* = (mu − mu_ref)/sigma, per Van den Bergh, "Comments on normalized
/// Elo" §4.1, as implemented by fastchess `getLLR_normalized`.
fn mle(probs: &[f64; 5], mu_ref: f64, t_star: f64) -> [f64; 5] {
    const THETA_EPSILON: f64 = 1e-7;
    const MLE_EPSILON: f64 = 1e-4;

    let mut p = [1.0 / 5.0; 5];
    for _ in 0..10 {
        let (mu, var) = mean_and_variance(&PAIR_SCORES, &p);
        let sigma = var.sqrt();
        let phi: [f64; 5] = std::array::from_fn(|i| {
            let a_i = PAIR_SCORES[i];
            a_i - mu_ref - 0.5 * t_star * sigma * (1.0 + ((a_i - mu) / sigma).powi(2))
        });

        let u = phi.iter().copied().fold(f64::INFINITY, f64::min);
        let v = phi.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let theta = itp(
            |x| (0..5).map(|i| probs[i] * phi[i] / (1.0 + x * phi[i])).sum(),
            -1.0 / v,
            -1.0 / u,
            f64::INFINITY,
            f64::NEG_INFINITY,
            0.1,
            2.0,
            0.99,
            THETA_EPSILON,
        );

        let mut max_diff = 0.0f64;
        for i in 0..5 {
            let new_p = probs[i] / (1.0 + theta * phi[i]);
            max_diff = max_diff.max((new_p - p[i]).abs());
            p[i] = new_p;
        }
        if max_diff < MLE_EPSILON {
            break;
        }
    }
    p
}

/// ITP root finder (Oliveira & Takahashi 2020), ported verbatim from
/// fastchess. The ±∞ initial f_a/f_b make the secant point NaN on early
/// iterations; NaN comparisons are false, so those iterations reduce to
/// plain bisection by construction — do not "fix" this.
#[allow(clippy::too_many_arguments)]
fn itp(
    f: impl Fn(f64) -> f64,
    a: f64,
    b: f64,
    f_a: f64,
    f_b: f64,
    k_1: f64,
    k_2: f64,
    n_0: f64,
    epsilon: f64,
) -> f64 {
    let (mut a, mut b, mut f_a, mut f_b) =
        if f_a > 0.0 { (b, a, f_b, f_a) } else { (a, b, f_a, f_b) };
    debug_assert!(f_a < 0.0 && 0.0 < f_b);

    let n_half = ((b - a).abs() / (2.0 * epsilon)).log2().ceil();
    let n_max = n_half + n_0;
    let mut i = 0.0;
    while (b - a).abs() > 2.0 * epsilon {
        let x_half = (a + b) / 2.0;
        let r = epsilon * 2f64.powf(n_max - i) - (b - a) / 2.0;
        let delta = k_1 * (b - a).powf(k_2);

        let x_f = (f_b * a - f_a * b) / (f_b - f_a);

        let sigma = (x_half - x_f) / (x_half - x_f).abs();
        let x_t = if delta <= (x_half - x_f).abs() { x_f + sigma * delta } else { x_half };

        let x_itp = if (x_t - x_half).abs() <= r { x_t } else { x_half - sigma * r };

        let f_itp = f(x_itp);
        if f_itp == 0.0 {
            a = x_itp;
            b = x_itp;
        } else if f_itp.is_sign_negative() {
            a = x_itp;
            f_a = f_itp;
        } else {
            b = x_itp;
            f_b = f_itp;
        }
        i += 1.0;
    }
    (a + b) / 2.0
}

/// Error function, Abramowitz–Stegun 7.1.26 (max abs error 1.5e-7 —
/// invisible at the 2 decimals LOS is displayed with). Rust std has no erf.
fn erf(x: f64) -> f64 {
    let t = 1.0 / (1.0 + 0.327_591_1 * x.abs());
    let poly = t
        * (0.254_829_592
            + t * (-0.284_496_736 + t * (1.421_413_741 + t * (-1.453_152_027 + t * 1.061_405_429))));
    let y = 1.0 - poly * (-x * x).exp();
    if x < 0.0 { -y } else { y }
}

// ---------------------------------------------------------------------------
// rendering

/// `{:.2}` for finite values, `n/a` (or `inf` for ratios) otherwise.
fn f2(x: f64) -> String {
    if x.is_finite() { format!("{x:.2}") } else { "n/a".into() }
}

fn ratio2(x: f64) -> String {
    if x.is_finite() {
        format!("{x:.2}")
    } else if x.is_infinite() {
        "inf".into()
    } else {
        "n/a".into()
    }
}

fn verdict_headline(v: Verdict) -> &'static str {
    match v {
        Verdict::AcceptH1 => "PASS (H1 accepted)",
        Verdict::AcceptH0 => "FAIL (H0 accepted)",
        Verdict::Inconclusive => "INCONCLUSIVE (no bound crossed)",
    }
}

fn inconclusive_reason(d: &RunData, a: &Analysis) -> &'static str {
    if d.rounds_cap > 0 && a.pairs >= d.rounds_cap {
        "hit its round cap"
    } else {
        "was stopped early (interrupted)"
    }
}

fn verdict_paragraph(d: &RunData, a: &Analysis) -> String {
    match a.verdict {
        Verdict::AcceptH1 => format!(
            "The match stopped because the log-likelihood ratio reached the upper bound \
             ({} >= {}). The games played are about {:.0}x better explained by H1 — \
             \"dev is at least {} nElo stronger than base\" — than by H0 — \"dev is at \
             most {} nElo stronger\". A change that is really no better than {} nElo had \
             at most a {:.0}-in-100 chance of passing like this. **The change passes this \
             test and is safe to keep.**",
            f2(a.llr),
            f2(a.upper),
            a.upper.exp(),
            f2(d.elo1),
            f2(d.elo0),
            f2(d.elo0),
            d.alpha * 100.0,
        ),
        Verdict::AcceptH0 => format!(
            "The match stopped because the log-likelihood ratio reached the lower bound \
             ({} <= {}). The games played are about {:.0}x better explained by H0 — \
             \"dev is at most {} nElo stronger than base\" — than by H1 — \"dev is at \
             least {} nElo stronger\". This does not prove the change made the engine \
             worse; it shows that whatever improvement exists is smaller than the {} nElo \
             the test was asked to certify. A genuinely good-enough change had at most a \
             {:.0}-in-100 chance of failing like this. **The change fails this test.**",
            f2(a.llr),
            f2(a.lower),
            (-a.lower).exp(),
            f2(d.elo0),
            f2(d.elo1),
            f2(d.elo1),
            d.beta * 100.0,
        ),
        Verdict::Inconclusive => {
            let (pct, bound) = if a.llr >= 0.0 {
                (100.0 * a.llr / a.upper, "upper")
            } else {
                (100.0 * a.llr / a.lower, "lower")
            };
            format!(
                "The match ended without the LLR crossing either bound: it {} at LLR {} \
                 — about {:.0}% of the way to the {} bound — between {} and {}. **There \
                 is no verdict.** The honest reading is \"not enough evidence yet\", not \
                 \"no difference\": run again with a higher --rounds cap, or test wider \
                 [elo0, elo1] bounds, to reach a decision with fewer games.",
                inconclusive_reason(d, a),
                f2(a.llr),
                pct,
                bound,
                f2(a.lower),
                f2(a.upper),
            )
        }
    }
}

fn render(d: &RunData, a: &Analysis, generated: &str) -> String {
    let p = &d.penta;
    let mut r = String::new();
    let out = &mut r;

    // -- header -------------------------------------------------------------
    out.push_str(&format!(
        "# SPRT report — {run}\n\n\
         **{pair}**: `dev` is the working tree, `base` is commit `{sha}`.\n\
         Generated {generated}. Regenerate any time with\n\
         `cargo xtask sprt-report {run}`.\n\n",
        run = d.run_name,
        pair = d.pair_name,
        sha = d.base_sha,
    ));

    // -- verdict ------------------------------------------------------------
    out.push_str(&format!(
        "## Verdict: {}\n\n{}\n\n",
        verdict_headline(a.verdict),
        verdict_paragraph(d, a),
    ));

    // -- the test at a glance -----------------------------------------------
    out.push_str(&format!(
        "## The test at a glance\n\n\
         | | |\n\
         |---|---|\n\
         | Match | `dev` (working tree) vs `base` (`{sha}`) |\n\
         | Time control | {tc} (seconds per game + seconds added per move, per side) |\n\
         | Openings | {book}, random order, each position played twice with colors swapped |\n\
         | Hypotheses | H0: dev gains <= {elo0} nElo · H1: dev gains >= {elo1} nElo |\n\
         | Error budget | alpha = {alpha} (false pass) · beta = {beta} (false fail) |\n\
         | Stopping rule | accept H1 when LLR >= {upper} · accept H0 when LLR <= {lower} |\n\
         | Round cap | {cap} pairs ({cap2} games) |\n\
         | Adjudication | {adj} |\n\n",
        sha = d.base_sha,
        tc = d.tc,
        book = d.book,
        elo0 = f2(d.elo0),
        elo1 = f2(d.elo1),
        alpha = d.alpha,
        beta = d.beta,
        upper = f2(a.upper),
        lower = f2(a.lower),
        cap = d.rounds_cap,
        cap2 = d.rounds_cap * 2,
        adj = d.adjudication,
    ));

    // -- result -------------------------------------------------------------
    out.push_str(&format!(
        "## Result\n\n\
         ```\n\
         LLR: {llr} ({lower}, {upper}) [{elo0}, {elo1}]\n\
         Elo: {elo} +/- {elo_err}, nElo: {nelo} +/- {nelo_err}\n\
         LOS: {los:.2} %, DrawRatio: {drp:.2} %, PairsRatio: {pr}\n\
         Games: {games}, Wins: {w}, Losses: {l}, Draws: {dr}, Points: {pts} ({sp:.2} %)\n\
         Ptnml(0-2): [{ll}, {ld}, {mid}, {wd}, {ww}], WL/DD Ratio: {wldd}\n\
         ```\n\n",
        llr = f2(a.llr),
        lower = f2(a.lower),
        upper = f2(a.upper),
        elo0 = f2(d.elo0),
        elo1 = f2(d.elo1),
        elo = f2(a.elo),
        elo_err = f2(a.elo_err),
        nelo = f2(a.nelo),
        nelo_err = f2(a.nelo_err),
        los = a.los * 100.0,
        drp = a.draw_pct_pairs,
        pr = ratio2(a.pairs_ratio),
        games = a.games,
        w = d.wins,
        l = d.losses,
        dr = d.draws,
        pts = a.points,
        sp = a.score_pct,
        ll = p.ll,
        ld = p.ld,
        mid = p.wl + p.dd,
        wd = p.wd,
        ww = p.ww,
        wldd = ratio2(a.wl_dd),
    ));

    // -- what each value means ----------------------------------------------
    let elo_meaning = if a.elo.is_finite() {
        format!(
            "Best estimate of the strength difference on the familiar rating scale; \
             the true value is ~95% likely to lie in [{:.1}, {:.1}]. An interval that \
             excludes 0 means a real difference, whatever the verdict.",
            a.elo - a.elo_err,
            a.elo + a.elo_err,
        )
    } else {
        "The score was 0% or 100%, which maps to an infinite Elo difference — \
         only happens in tiny or completely one-sided samples."
            .into()
    };
    out.push_str(&format!(
        "### What each value means\n\n\
         | Metric | Value | Meaning |\n\
         |---|---|---|\n\
         | LLR | {llr} in ({lower}, {upper}) | The evidence meter. Starts at 0; each pair \
           of games nudges it up (toward \"dev is stronger\") or down. Crossing a bound \
           ends the match with a verdict. |\n\
         | Elo | {elo} +/- {elo_err} | {elo_meaning} |\n\
         | nElo | {nelo} +/- {nelo_err} | The same difference in draw-rate-adjusted units \
           — the scale the [{elo0}, {elo1}] bounds are written in (see glossary). |\n\
         | LOS | {los:.2} % | Probability that dev is stronger than base *at all*. Says \
           nothing about *by how much* — that is the bounds' job. |\n\
         | Score | {sp:.2} % ({pts} of {games}) | Points per game (win = 1, draw = 0.5); \
           50% = equal strength. |\n\
         | Games / pairs | {games} / {pairs} | Every opening is played twice with colors \
           swapped; the pair is the statistical unit. |\n\
         | W-D-L | {w}-{dr}-{l} | Raw game outcomes from dev's point of view. |\n\
         | Ptnml | [{ll}, {ld}, {mid}, {wd}, {ww}] | Pairs by points scored [0, 0.5, 1, \
           1.5, 2]: dev swept both games of a pair {ww}x and lost both {ll}x. |\n\
         | DrawRatio | {drp:.2} % pairs, {drg:.2} % games | Share of pairs split 1-1, and \
           of individual games drawn. High values are normal at fast time controls. |\n\
         | PairsRatio | {pr} | Winning pairs (WD+WW) per losing pair (LD+LL); above 1 \
           favors dev. |\n\
         | WL/DD | {wldd} | How the 1-1 pairs split: win+loss vs two draws. High values \
           hint at sharp openings or volatile play rather than quiet equality. |\n\n",
        llr = f2(a.llr),
        lower = f2(a.lower),
        upper = f2(a.upper),
        elo = f2(a.elo),
        elo_err = f2(a.elo_err),
        elo_meaning = elo_meaning,
        nelo = f2(a.nelo),
        nelo_err = f2(a.nelo_err),
        elo0 = f2(d.elo0),
        elo1 = f2(d.elo1),
        los = a.los * 100.0,
        sp = a.score_pct,
        pts = a.points,
        games = a.games,
        pairs = a.pairs,
        w = d.wins,
        dr = d.draws,
        l = d.losses,
        ll = p.ll,
        ld = p.ld,
        mid = p.wl + p.dd,
        wd = p.wd,
        ww = p.ww,
        drp = a.draw_pct_pairs,
        drg = a.draw_pct_games,
        pr = ratio2(a.pairs_ratio),
        wldd = ratio2(a.wl_dd),
    ));

    // -- fine print ----------------------------------------------------------
    out.push_str(
        "## Fine print\n\n\
         All numbers are recomputed from the final tallies in `config.json` using the \
         same formulas as fastchess (exact GSPRT with maximum-likelihood estimates, \
         normalized model). They can differ in the last digits from the live values \
         fastchess printed, because games that finished while it was shutting down are \
         included here. An LOS of 100.00% is rounding — it is never literally 1.\n\n",
    );

    // -- glossary ------------------------------------------------------------
    out.push_str(&glossary(d, a));
    r
}

/// The educational section. Static prose with the run's own numbers woven in.
fn glossary(d: &RunData, a: &Analysis) -> String {
    // Near equality, 1 nElo corresponds to 2·sqrt(2·var) logistic Elo at this
    // run's draw rate — lets the reader translate the bounds into familiar units.
    let elo_per_nelo = 2.0 * (2.0 * a.pair_var).sqrt();
    let bounds_in_elo = if elo_per_nelo.is_finite() && elo_per_nelo > 0.0 {
        format!(
            " At this run's draw rate 1 nElo ~ {:.2} classic Elo near equality, so the \
             [{}, {}] nElo bounds correspond to roughly [{:.1}, {:.1}] classic Elo.",
            elo_per_nelo,
            f2(d.elo0),
            f2(d.elo1),
            d.elo0 * elo_per_nelo,
            d.elo1 * elo_per_nelo,
        )
    } else {
        String::new()
    };

    format!(
        "## For the uninitiated\n\n\
         **SPRT — Sequential Probability Ratio Test.** The naive way to compare two \
         versions of an engine is to fix a number of games up front, play them all, and \
         look at the score. That wastes time when the outcome is clear early, and it \
         breaks statistically if you peek at the score and stop when you like it — the \
         error rates of a fixed-length test are only valid if you never peek. The SPRT \
         (Wald, 1945) is built the other way around: it re-weighs the evidence after \
         every game pair and stops the moment the evidence is decisive, and for chosen \
         error rates it needs, on average, fewer games than any other test. Stockfish's \
         Fishtest and essentially every serious engine project gate changes this way.\n\n\
         **H0, H1 and the bounds (elo0 = {elo0}, elo1 = {elo1}).** The test weighs two \
         competing claims: H0 \"the change is worth at most {elo0} nElo\" against H1 \
         \"it is worth at least {elo1} nElo\". The gap between them is a deliberate gray \
         zone: if the truth lies inside it, either verdict is considered acceptable. The \
         narrower the gap, the more games a decision costs (roughly proportional to \
         1/gap^2), so the bounds encode the smallest improvement worth waiting to \
         detect. Note: with fastchess's `model=normalized` these bounds are in \
         *normalized* Elo, not the classic Elo of rating lists (see below).\n\n\
         **alpha = {alpha}, beta = {beta} — the error budget.** alpha is the probability \
         that a change no better than elo0 still passes (false positive); beta is the \
         probability that a change as good as elo1 still fails (false miss). At 0.05 \
         each, both mistakes happen about 1 time in 20 — accepted, because the \
         alternative (near-zero error rates) would cost enormously many games.\n\n\
         **LLR — log-likelihood ratio.** The running score of the evidence: the \
         logarithm of how much better H1 explains the observed pair outcomes than H0. \
         Zero means \"no idea yet\"; positive favors H1. The match stops at \
         ln((1-beta)/alpha) = {upper} or ln(beta/(1-alpha)) = {lower}; crossing the \
         upper bound literally means the games are e^{upper} ~ {odds:.0} times more \
         probable under H1 than under H0 — the agreed standard of proof. Until then the \
         LLR wanders like a random walk, and wandering *near* a bound proves nothing. \
         (This runner recomputes the LLR with the exact generalized-SPRT formula — \
         maximum-likelihood distributions constrained to each hypothesis — identical to \
         fastchess's own implementation.)\n\n\
         **Game pairs and opening bias.** Opening books contain sharp, unbalanced \
         positions; whoever happens to sit on the strong side wins \"for free\". Every \
         opening is therefore played twice with colors swapped (`-repeat`), so book \
         bias cancels within the pair — and the *pair*, not the game, is the unit of \
         evidence: 0, 0.5, 1, 1.5 or 2 points.\n\n\
         **Pentanomial counting (the Ptnml row).** Counts pairs by those five outcomes: \
         [LL, LD, WL+DD, WD, WW]. The two games of a pair share an opening (and often a \
         result), so they are statistically correlated; treating all games as \
         independent (the \"trinomial\" W/D/L view) underestimates the variance and \
         produces overconfident error bars. The pentanomial view measures the \
         correlation and keeps the statistics honest.\n\n\
         **Logistic Elo vs normalized Elo.** Logistic Elo is the familiar scale: a score \
         of 64% ~ +100 Elo. But how many games a given Elo edge takes to *detect* \
         depends on the draw rate — at 90% draws a small edge hides much longer than at \
         30%. Normalized Elo divides the score offset by its observed standard \
         deviation instead, so a 5-nElo edge costs about the same number of games to \
         detect at any time control or draw rate. That predictability is why the SPRT \
         bounds are set in nElo.{bounds_in_elo}\n\n\
         **LOS — likelihood of superiority.** The probability, given the observed \
         score and spread, that dev is stronger than base at all (i.e. that the true \
         difference is above zero). Even 95% LOS is far weaker than what the SPRT \
         demands before declaring a pass, which is why tests are not stopped on LOS.\n\n\
         **Draw ratios.** {drg:.2}% of games were drawn and {drp:.2}% of pairs were \
         split 1-1. High draw rates are normal at fast time controls with balanced \
         books; they carry little evidence either way and simply slow the test down. \
         The WL/DD ratio splits the 1-1 pairs into win+loss versus two quiet draws.\n\n\
         **Time control {tc}.** Seconds per game plus seconds added per move, per side. \
         Fast controls maximize games per hour, and results usually generalize — but \
         changes to time management itself deserve a confirmation run at a slower \
         control.\n\n\
         **Adjudication.** To save time the runner ends foregone games early: {adj}. \
         These rules only call games whose outcome is no longer in doubt.\n\n\
         **Files in this folder.** `config.json` — the full fastchess configuration \
         plus the raw tallies; this report is computed from it. `games.pgn` — every \
         game, replayable in any chess GUI. `report.md` — this file; regenerate it with \
         `cargo xtask sprt-report {run}`.\n",
        elo0 = f2(d.elo0),
        elo1 = f2(d.elo1),
        alpha = d.alpha,
        beta = d.beta,
        upper = f2(a.upper),
        lower = f2(a.lower),
        odds = a.upper.exp(),
        bounds_in_elo = bounds_in_elo,
        drg = a.draw_pct_games,
        drp = a.draw_pct_pairs,
        tc = d.tc,
        adj = d.adjudication,
        run = d.run_name,
    )
}

fn print_summary(d: &RunData, a: &Analysis) {
    let reason = match a.verdict {
        Verdict::AcceptH1 => "LLR crossed the upper bound".to_string(),
        Verdict::AcceptH0 => "LLR crossed the lower bound".to_string(),
        Verdict::Inconclusive => format!("LLR {} never crossed a bound", f2(a.llr)),
    };
    println!("[sprt] -- result ------------------------------------------------");
    println!("[sprt] verdict : {} — {}", verdict_headline(a.verdict), reason);
    println!(
        "[sprt] LLR     : {}  bounds ({}, {})",
        f2(a.llr),
        f2(a.lower),
        f2(a.upper)
    );
    println!(
        "[sprt] Elo     : {} +/- {}   nElo {} +/- {}   LOS {:.2}%",
        f2(a.elo),
        f2(a.elo_err),
        f2(a.nelo),
        f2(a.nelo_err),
        a.los * 100.0
    );
    println!(
        "[sprt] games   : {} ({} pairs)   W-D-L {}-{}-{}   draws {:.2}% of games",
        a.games, a.pairs, d.wins, d.draws, d.losses, a.draw_pct_games
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(actual: f64, expected: f64, tol: f64) {
        assert!(
            (actual - expected).abs() < tol,
            "expected {expected}, got {actual} (tol {tol})"
        );
    }

    fn run_data(penta: Penta, wins: u64, draws: u64, losses: u64) -> RunData {
        RunData {
            run_name: "test".into(),
            pair_name: "dev vs base".into(),
            base_sha: "abc".into(),
            wins,
            losses,
            draws,
            penta,
            elo0: 0.0,
            elo1: 5.0,
            alpha: 0.05,
            beta: 0.05,
            model: "normalized".into(),
            rounds_cap: 20000,
            tc: "8+0.08".into(),
            book: "openings.epd".into(),
            adjudication: "none".into(),
        }
    }

    #[test]
    fn erf_reference_values() {
        assert_close(erf(0.0), 0.0, 1.5e-7);
        assert_close(erf(1.0), 0.842_700_792_949_715, 1.5e-7);
        assert_close(erf(-1.0), -0.842_700_792_949_715, 1.5e-7);
        assert_close(erf(2.0), 0.995_322_265_018_953, 1.5e-7);
    }

    #[test]
    fn llr_bounds_are_ln_19() {
        let d = run_data(Penta { ww: 1, wd: 0, wl: 0, dd: 0, ld: 0, ll: 0 }, 2, 0, 0);
        let a = analyze(&d).unwrap();
        assert_close(a.upper, 2.944_438_979_166_440_3, 1e-12);
        assert_close(a.lower, -2.944_438_979_166_440_3, 1e-12);
    }

    /// The real run on disk: target/sprt/runs/20260722-013620-vs-a88e5c18b030.
    #[test]
    fn real_run_analysis() {
        let d = run_data(Penta { ww: 56, wd: 108, wl: 35, dd: 43, ld: 31, ll: 5 }, 255, 225, 76);
        let a = analyze(&d).unwrap();

        assert_eq!(a.pairs, 278);
        assert_eq!(a.games, 556);
        assert_close(a.points, 367.5, 1e-9);
        assert_close(a.pair_score, 0.660_971_223_021_582_8, 1e-9);
        assert_close(a.pair_var, 0.060_194_380_466_85, 1e-9);
        assert_close(a.llr, 2.950_272_364_726_299_4, 1e-6);
        assert_close(a.elo, 115.978_395_551_360_88, 1e-6);
        assert_close(a.elo_err, 22.398_407_767_147_503, 1e-6);
        assert_close(a.nelo, 161.186_835_732_256_44, 1e-6);
        assert_close(a.nelo_err, 28.879_189_035_253_944, 1e-6);
        assert_close(a.los, 1.0, 1e-7);
        assert_close(a.draw_pct_games, 40.467_625_899_3, 1e-9);
        assert_close(a.draw_pct_pairs, 28.057_553_956_8, 1e-9);
        assert_close(a.pairs_ratio, 4.555_555_555_6, 1e-9);
        assert_close(a.wl_dd, 0.813_953_488_4, 1e-9);
        assert_eq!(a.verdict, Verdict::AcceptH1);
    }

    /// Cross-checks against fastchess's own test suite (sprt_test.cpp).
    #[test]
    fn llr_upstream_cases() {
        let llr = |ll, ld, wl, dd, wd, ww, e0, e1| {
            llr_penta(Penta { ww, wd, wl, dd, ld, ll }, e0, e1)
        };
        assert_close(llr(365, 16618, 36029, 200, 16974, 390, 0.0, 2.0), 2.250_367_703_545_846, 1e-6);
        assert_close(llr(127, 4883, 10311, 401, 5150, 104, -1.75, 0.25), 3.010_154_204_801_971_4, 1e-6);
        assert_close(llr(0, 0, 0, 0, 0, 5550, 0.0, 5.0), 111.797_555_086_713_64, 1e-6);
    }

    /// Cross-check against fastchess's elo_test.cpp pentanomial case.
    #[test]
    fn elo_upstream_case() {
        let d = run_data(Penta { ww: 334, wd: 333, wl: 457, dd: 41, ld: 433, ll: 332 }, 0, 0, 0);
        let a = analyze(&d).unwrap();
        assert_close(a.pair_score, 0.487_564_766_839_378_27, 1e-9);
        assert_close(a.elo, -8.642_667_267_969_01, 1e-6);
        assert_close(a.elo_err, 10.334_194_424_393_02, 1e-6);
        assert_close(a.nelo, -9.172_914_270_047_135, 1e-6);
        assert_close(a.nelo_err, 10.960_458_877_511_137, 1e-6);
        assert_close(a.los, 0.050_470_054_432_449_674, 1e-7);
    }

    #[test]
    fn analyze_rejects_empty_and_foreign_models() {
        let empty = run_data(Penta { ww: 0, wd: 0, wl: 0, dd: 0, ld: 0, ll: 0 }, 0, 0, 0);
        assert!(analyze(&empty).unwrap_err().contains("no completed game pairs"));

        let mut logistic = run_data(Penta { ww: 1, wd: 0, wl: 0, dd: 0, ld: 0, ll: 0 }, 2, 0, 0);
        logistic.model = "logistic".into();
        assert!(analyze(&logistic).unwrap_err().contains("model"));
    }

    const MINIMAL: &str = r#"{
        "sprt": {"alpha": 0.05, "beta": 0.05, "elo0": 0.0, "elo1": 5.0, "model": "normalized"},
        "rounds": 100,
        "stats": {"dev vs base": {"wins": 10, "losses": 5, "draws": 5,
            "penta_WW": 2, "penta_WD": 3, "penta_WL": 1, "penta_DD": 2, "penta_LD": 1, "penta_LL": 1}}
    }"#;

    #[test]
    fn parse_config_minimal() {
        let d = parse_config(MINIMAL, "20260101-000000-vs-abcdef123456").unwrap();
        assert_eq!(d.pair_name, "dev vs base");
        assert_eq!((d.wins, d.draws, d.losses), (10, 5, 5));
        assert_eq!(d.penta.pairs(), 10);
        assert_eq!(d.rounds_cap, 100);
        assert_eq!(d.base_sha, "abcdef123456"); // from the run name, no engines section
        assert_eq!(d.tc, "?");
        assert_eq!(d.adjudication, "none");
    }

    #[test]
    fn parse_config_stats_key_fallback() {
        let text = MINIMAL.replace("dev vs base", "alpha vs beta");
        let d = parse_config(&text, "run").unwrap();
        assert_eq!(d.pair_name, "alpha vs beta");
    }

    #[test]
    fn parse_config_errors() {
        let no_penta = MINIMAL.replace("penta_WW", "penta_XX");
        assert!(parse_config(&no_penta, "run").unwrap_err().contains("penta_WW"));

        let no_sprt = MINIMAL.replace("\"sprt\"", "\"other\"");
        assert!(parse_config(&no_sprt, "run").unwrap_err().contains("sprt"));

        let no_stats = MINIMAL.replace("\"stats\"", "\"other\"");
        assert!(parse_config(&no_stats, "run").unwrap_err().contains("no game statistics"));
    }
}
