use crate::util::{Result, cargo, run};

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

/// Pull `--hash MB` out of an argument list, leaving the positionals behind.
pub fn split_hash(args: &[String]) -> (Option<&str>, Vec<&str>) {
    let mut hash = None;
    let mut rest = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--hash" => hash = it.next().map(String::as_str),
            other => rest.push(other),
        }
    }
    (hash, rest)
}

pub fn search_bench(depth: Option<&str>, hash: Option<&str>) -> Result<()> {
    let mut cmd = cargo();
    cmd.args(["run", "--release", "--quiet", "--", "searchbench"]);
    if let Some(d) = depth {
        cmd.arg(d);
    }
    if let Some(h) = hash {
        cmd.args(["--hash", h]);
    }
    run(&mut cmd)
}
