use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::sprt_report;
use crate::util::{Result, WorktreeGuard, cargo, git, run, run_capture, workspace_root};

pub struct SprtConfig {
    pub gitref: String,
    pub elo0: String,
    pub elo1: String,
    pub tc: String,
    pub concurrency: String,
    pub rounds: String,
    pub book: Option<PathBuf>,
}

pub fn default_concurrency() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get() / 2)
        .unwrap_or(1)
        .max(1)
}

impl Default for SprtConfig {
    fn default() -> Self {
        Self {
            gitref: "HEAD".into(),
            elo0: "0".into(),
            elo1: "5".into(),
            tc: "8+0.08".into(),
            concurrency: default_concurrency().to_string(),
            rounds: "20000".into(),
            book: None,
        }
    }
}

pub fn sprt(cfg: &SprtConfig) -> Result<()> {
    let root = workspace_root();

    let sha = run_capture(git().args(["rev-parse", "--short=12", &cfg.gitref]))
        .map_err(|e| format!("cannot resolve ref '{}': {e}", cfg.gitref))?;

    let book = cfg
        .book
        .clone()
        .unwrap_or_else(|| root.join("xtask/books/openings.epd"));
    if !book.is_file() {
        return Err(format!(
            "opening book not found: {}\n\
             SPRT needs varied openings; games all starting from the start position \
             would be correlated and the result meaningless.\n\
             Download a balanced EPD book (e.g. noob_3moves.epd from \
             https://github.com/official-stockfish/books), place it at \
             xtask/books/openings.epd, or pass --book <path>.",
            book.display()
        ));
    }

    if Command::new("fastchess").arg("--version").output().is_err() {
        return Err(
            "fastchess not found on PATH — install it from https://github.com/Disservin/fastchess"
                .into(),
        );
    }

    let sprt_dir = root.join("target/sprt");
    std::fs::create_dir_all(&sprt_dir).map_err(|e| format!("cannot create target/sprt: {e}"))?;

    // Binaries are cached in target/sprt and shared across runs; the per-run
    // outputs (config, PGN) live in their own timestamped folder so runs don't
    // clobber each other.
    let stamp = run_capture(Command::new("date").arg("+%Y%m%d-%H%M%S"))
        .unwrap_or_else(|_| "run".into());
    let run_name = format!("{stamp}-vs-{sha}");
    let run_dir = sprt_dir.join("runs").join(&run_name);
    std::fs::create_dir_all(&run_dir)
        .map_err(|e| format!("cannot create {}: {e}", run_dir.display()))?;
    // fastchess runs with cwd = root, so its output args must be root-relative.
    let run_rel = format!("target/sprt/runs/{run_name}");

    // Children (cargo, fastchess) receive SIGINT with the process group and die
    // on their own; we just note the interrupt so error messages make sense and
    // unwind normally, which runs the worktree guard.
    let interrupted = Arc::new(AtomicBool::new(false));
    let _ = ctrlc::set_handler({
        let interrupted = Arc::clone(&interrupted);
        move || interrupted.store(true, Ordering::SeqCst)
    });
    let check_interrupt = |r: Result<()>| -> Result<()> {
        if interrupted.load(Ordering::SeqCst) {
            Err("interrupted".into())
        } else {
            r
        }
    };

    println!("[sprt] building dev engine (working tree)...");
    check_interrupt(run(cargo().args(["build", "--release"])))?;
    let dev_bin = sprt_dir.join("mythos-dev");
    std::fs::copy(root.join("target/release/mythos"), &dev_bin)
        .map_err(|e| format!("cannot copy dev binary: {e}"))?;

    let base_bin = sprt_dir.join(format!("mythos-base-{sha}"));
    if base_bin.is_file() {
        println!("[sprt] reusing cached baseline binary for {sha}");
    } else {
        println!("[sprt] building baseline engine ({} @ {sha})...", cfg.gitref);
        let wt_dir = sprt_dir.join(format!("worktree-{sha}"));
        if wt_dir.exists() {
            // Leftover from a crashed run.
            let _ = git()
                .args(["worktree", "remove", "--force"])
                .arg(&wt_dir)
                .status();
            let _ = git().args(["worktree", "prune"]).status();
        }
        let guard = WorktreeGuard { dir: wt_dir.clone() };
        run(git()
            .args(["worktree", "add", "--detach"])
            .arg(&wt_dir)
            .arg(&sha))?;
        check_interrupt(run(cargo()
            .current_dir(&wt_dir)
            .args(["build", "--release"])))?;
        std::fs::copy(wt_dir.join("target/release/mythos"), &base_bin)
            .map_err(|e| format!("cannot copy baseline binary: {e}"))?;
        drop(guard);
    }

    println!(
        "[sprt] dev (working tree) vs base ({sha}), tc {}, sprt [{}, {}], concurrency {}",
        cfg.tc, cfg.elo0, cfg.elo1, cfg.concurrency
    );
    let result = check_interrupt(run(Command::new("fastchess")
        .current_dir(root)
        .args(["-engine", &format!("cmd={}", dev_bin.display()), "name=dev"])
        .args(["-engine", &format!("cmd={}", base_bin.display()), "name=base"])
        .args(["-each", &format!("tc={}", cfg.tc), "timemargin=50"])
        .args([
            "-openings",
            &format!("file={}", book.display()),
            "format=epd",
            "order=random",
        ])
        .args([
            "-sprt",
            &format!("elo0={}", cfg.elo0),
            &format!("elo1={}", cfg.elo1),
            "alpha=0.05",
            "beta=0.05",
            "model=normalized",
        ])
        .args(["-rounds", &cfg.rounds, "-repeat"])
        .args(["-concurrency", &cfg.concurrency])
        .args(["-maxmoves", "200"])
        .args(["-draw", "movenumber=40", "movecount=8", "score=10"])
        .args(["-resign", "movecount=4", "score=600", "twosided=true"])
        .args(["-recover", "-ratinginterval", "10", "-autosaveinterval", "0"])
        // fastchess saves its session config on exit; keep it out of the repo root.
        .args(["-config", &format!("outname={run_rel}/config.json")])
        .args(["-pgnout", &format!("file={run_rel}/games.pgn"), "notation=san"])));

    // fastchess writes config.json (with the tallies so far) on exit, even
    // after Ctrl-C, so a report is generated on the interrupt path too. A
    // report failure must never mask the match result.
    match sprt_report::generate(&run_dir) {
        Ok(path) => println!("[sprt] report: {}", path.display()),
        Err(e) => eprintln!("[sprt] warning: no report generated: {e}"),
    }
    println!("[sprt] baseline was {sha}; results saved to {run_rel}/");
    result
}
