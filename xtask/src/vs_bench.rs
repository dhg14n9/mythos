use std::path::{Path, PathBuf};
use std::process::Command;

use crate::util::{Result, WorktreeGuard, cargo, git, run, run_capture, workspace_root};

/// One per-position line of `searchbench` output.
struct Row {
    nodes: u64,
    time: f64,
    best: String,
    fen: String,
}

fn group_digits(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    let len = s.len();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

fn parse_rows(output: &str) -> Vec<Row> {
    let mut rows = Vec::new();
    for line in output.lines() {
        let toks: Vec<&str> = line.split_whitespace().collect();
        let find = |key: &str| toks.iter().position(|t| *t == key);
        let (Some(n), Some(t), Some(b)) = (find("nodes"), find("time"), find("bestmove")) else {
            continue;
        };
        let (Some(nodes), Some(time), Some(best)) = (toks.get(n + 1), toks.get(t + 1), toks.get(b + 1))
        else {
            continue;
        };
        let (Ok(nodes), Ok(time)) = (
            nodes.replace(',', "").parse(),
            time.trim_end_matches('s').parse(),
        ) else {
            continue;
        };
        if toks.len() <= b + 2 {
            continue;
        }
        rows.push(Row {
            nodes,
            time,
            best: best.to_string(),
            fen: toks[b + 2..].join(" "),
        });
    }
    rows
}

fn run_bench(bin: &Path, depth: &str, label: &str) -> Result<Vec<Row>> {
    println!("[vs] running {label} searchbench (depth {depth})...");
    let out = run_capture(
        Command::new(bin)
            .current_dir(workspace_root())
            .args(["searchbench", depth]),
    )?;
    let rows = parse_rows(&out);
    if rows.is_empty() {
        return Err(format!(
            "{label}: no per-position lines found in searchbench output \
             (does that build support `searchbench`?)"
        ));
    }
    Ok(rows)
}

/// Build the engine at `gitref` in a throwaway worktree, caching the binary
/// per commit under target/vsbench/.
fn build_ref_binary(gitref: &str) -> Result<(PathBuf, String)> {
    let root = workspace_root();
    let sha = run_capture(git().args(["rev-parse", "--short=12", gitref]))
        .map_err(|e| format!("cannot resolve ref '{gitref}': {e}"))?;

    let dir = root.join("target/vsbench");
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create target/vsbench: {e}"))?;

    let bin = dir.join(format!("mythos-{sha}"));
    if bin.is_file() {
        println!("[vs] reusing cached base binary for {sha}");
        return Ok((bin, sha));
    }

    println!("[vs] building base engine ({gitref} @ {sha})...");
    let wt_dir = dir.join(format!("worktree-{sha}"));
    if wt_dir.exists() {
        // Leftover from a crashed run.
        let _ = git()
            .args(["worktree", "remove", "--force"])
            .arg(&wt_dir)
            .status();
        let _ = git().args(["worktree", "prune"]).status();
    }
    let _guard = WorktreeGuard { dir: wt_dir.clone() };
    run(git()
        .args(["worktree", "add", "--detach"])
        .arg(&wt_dir)
        .arg(&sha))?;
    run(cargo().current_dir(&wt_dir).args(["build", "--release"]))?;
    std::fs::copy(wt_dir.join("target/release/mythos"), &bin)
        .map_err(|e| format!("cannot copy base binary: {e}"))?;
    Ok((bin, sha))
}

/// Run `searchbench` on the working tree and on a git ref, and diff the
/// per-position node counts and best moves.
pub fn vs_search_bench(gitref: &str, depth: &str) -> Result<()> {
    let root = workspace_root();

    let (base_bin, sha) = build_ref_binary(gitref)?;

    println!("[vs] building dev engine (working tree)...");
    run(cargo().args(["build", "--release"]))?;
    let dev_bin = root.join("target/release/mythos");

    let base = run_bench(&base_bin, depth, &format!("base ({sha})"))?;
    let dev = run_bench(&dev_bin, depth, "dev")?;

    println!();
    println!(
        "{:>3}  {:>13}  {:>13}  {:>8}  {:<14}  fen",
        "pos", "base nodes", "dev nodes", "diff", "bestmove"
    );

    let mut base_total = 0u64;
    let mut dev_total = 0u64;
    let mut best_changes = 0usize;
    for (i, d) in dev.iter().enumerate() {
        let Some(b) = base.iter().find(|b| b.fen == d.fen) else {
            println!("{:>3}  {:>13}  {:>13}  {:>8}  {:<14}  {}", i + 1, "-", group_digits(d.nodes), "new", d.best, d.fen);
            continue;
        };
        base_total += b.nodes;
        dev_total += d.nodes;

        let diff = if d.nodes == b.nodes {
            "=".to_string()
        } else {
            format!("{:+.2}%", (d.nodes as f64 - b.nodes as f64) / b.nodes as f64 * 100.0)
        };
        let best = if d.best == b.best {
            d.best.clone()
        } else {
            best_changes += 1;
            format!("{} -> {}", b.best, d.best)
        };
        println!(
            "{:>3}  {:>13}  {:>13}  {:>8}  {:<14}  {}",
            i + 1,
            group_digits(b.nodes),
            group_digits(d.nodes),
            diff,
            best,
            d.fen
        );
    }

    let base_time: f64 = base.iter().map(|r| r.time).sum();
    let dev_time: f64 = dev.iter().map(|r| r.time).sum();
    let node_diff = (dev_total as f64 - base_total as f64) / (base_total as f64).max(1.0) * 100.0;

    println!();
    println!("  base      : {gitref} @ {sha}");
    println!("  positions : {} (depth {depth})", dev.len());
    println!(
        "  nodes     : base {}  dev {}  ({node_diff:+.2}%)",
        group_digits(base_total),
        group_digits(dev_total)
    );
    println!(
        "  time      : base {base_time:.3}s  dev {dev_time:.3}s  ({:+.2}%)",
        (dev_time - base_time) / base_time.max(f64::EPSILON) * 100.0
    );
    println!(
        "  speed     : base {:.2} Mnps  dev {:.2} Mnps",
        base_total as f64 / base_time.max(f64::EPSILON) / 1e6,
        dev_total as f64 / dev_time.max(f64::EPSILON) / 1e6
    );
    println!("  bestmove  : {best_changes} changed");
    Ok(())
}
