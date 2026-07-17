use std::io::Write;
use std::process::Stdio;

use crate::util::{Result, STARTPOS, cargo, run};

/// Parse `--tt`-style switches out of an argument list, returning (tt, rest).
pub fn split_tt(args: &[String]) -> (bool, Vec<&str>) {
    let mut tt = false;
    let mut rest = Vec::new();
    for a in args {
        match a.as_str() {
            "--tt" | "tt" | "on" => tt = true,
            "--no-tt" | "off" => tt = false,
            other => rest.push(other),
        }
    }
    (tt, rest)
}

pub fn test(filter: Option<&str>) -> Result<()> {
    let mut cmd = cargo();
    cmd.arg("test");
    if let Some(f) = filter.filter(|f| !f.is_empty()) {
        cmd.arg(f);
    }
    run(&mut cmd)
}

pub fn perft() -> Result<()> {
    run(cargo().args(["test", "perft_suite"]))
}

pub fn perft_deep() -> Result<()> {
    run(cargo().args(["test", "perft_suite_deep", "--", "--ignored", "--nocapture"]))
}

pub fn bench() -> Result<()> {
    run(cargo().args(["test", "bench_make_unmake", "--", "--ignored", "--nocapture"]))
}

pub fn perft_bench(tt: bool, fen: Option<&str>, depth: Option<&str>) -> Result<()> {
    // --exact, or the substring filter would also match perft_bench_suite.
    run(cargo()
        .env("PERFT_FEN", fen.unwrap_or(STARTPOS))
        .env("PERFT_DEPTH", depth.unwrap_or("6"))
        .env("PERFT_TT", if tt { "1" } else { "0" })
        .args([
            "test",
            "--",
            "tests::perft::perft_bench",
            "--exact",
            "--ignored",
            "--nocapture",
        ]))
}

pub fn bench_suite(tt: bool) -> Result<()> {
    let mut cmd = cargo();
    cmd.args(["run", "--release", "--quiet", "--", "bench"]);
    if tt {
        cmd.arg("tt");
    }
    run(&mut cmd)
}

pub fn divide(fen: Option<&str>, depth: Option<&str>) -> Result<()> {
    let fen = fen.unwrap_or(STARTPOS);
    let depth = depth.unwrap_or("1");

    let mut child = cargo()
        .args(["run", "--release", "--quiet"])
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to run cargo: {e}"))?;

    child
        .stdin
        .take()
        .expect("stdin was piped")
        .write_all(format!("position fen {fen}\ngo perft {depth}\nquit\n").as_bytes())
        .map_err(|e| format!("failed to write to engine stdin: {e}"))?;

    let status = child
        .wait()
        .map_err(|e| format!("failed to wait for engine: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("engine exited with {status}"))
    }
}
