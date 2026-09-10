mod menu;
mod release;
mod sprt;
mod sprt_report;
mod tasks;
mod util;
mod vs_bench;

use sprt::SprtConfig;
use util::Result;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Err(e) = dispatch(&args) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn dispatch(args: &[String]) -> Result<()> {
    let Some((cmd, rest)) = args.split_first() else {
        return menu::menu();
    };

    match cmd.as_str() {
        "test" => tasks::test(rest.first().map(String::as_str)),
        "perft" => tasks::perft(),
        "perft-deep" => tasks::perft_deep(),
        "perft-bench" => {
            let (tt, pos) = tasks::split_tt(rest);
            tasks::perft_bench(tt, pos.first().copied(), pos.get(1).copied())
        }
        "bench-suite" => {
            let (tt, _) = tasks::split_tt(rest);
            tasks::bench_suite(tt)
        }
        "divide" => tasks::divide(
            rest.first().map(String::as_str),
            rest.get(1).map(String::as_str),
        ),
        "bench" => tasks::bench(),
        "search-bench" => {
            let (hash, pos) = tasks::split_hash(rest);
            tasks::search_bench(pos.first().copied(), hash)
        }
        "vs-search-bench" => {
            let (hash, pos) = tasks::split_hash(rest);
            let (gitref, depth) = parse_vs_args(&pos)?;
            vs_bench::vs_search_bench(gitref, depth, hash)
        }
        "release" => release::release(parse_release_flags(rest)?),
        "sprt" => sprt::sprt(&parse_sprt_flags(rest)?),
        "sprt-report" => match rest {
            [dir] => sprt_report::report_cmd(dir),
            _ => Err("usage: cargo xtask sprt-report <run-dir>".into()),
        },
        "help" | "-h" | "--help" => {
            print!("{USAGE}");
            Ok(())
        }
        other => Err(format!("unknown command: {other}\n\n{USAGE}")),
    }
}

/// `vs-search-bench [ref] [depth]` in either order: a purely numeric
/// argument is the depth, anything else is the git ref.
fn parse_vs_args<'a>(args: &[&'a str]) -> Result<(&'a str, &'a str)> {
    let mut gitref = "HEAD";
    let mut depth = "7";
    for &a in args {
        if a.chars().all(|c| c.is_ascii_digit()) {
            depth = a;
        } else {
            gitref = a;
        }
    }
    Ok((gitref, depth))
}

fn parse_release_flags(args: &[String]) -> Result<bool> {
    let mut nopext = false;
    for a in args {
        match a.as_str() {
            "--nopext" => nopext = true,
            other => return Err(format!("unknown release flag: {other}")),
        }
    }
    Ok(nopext)
}

fn parse_sprt_flags(args: &[String]) -> Result<SprtConfig> {
    let mut cfg = SprtConfig::default();
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let mut value = |name: &str| -> Result<String> {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match flag.as_str() {
            "--ref" => cfg.gitref = value("--ref")?,
            "--elo0" => cfg.elo0 = value("--elo0")?,
            "--elo1" => cfg.elo1 = value("--elo1")?,
            "--tc" => cfg.tc = value("--tc")?,
            "--concurrency" => cfg.concurrency = value("--concurrency")?,
            "--rounds" => cfg.rounds = value("--rounds")?,
            "--book" => cfg.book = Some(value("--book")?.into()),
            "--affinity" => cfg.affinity = Some(value("--affinity")?),
            other => return Err(format!("unknown sprt flag: {other}")),
        }
    }
    Ok(cfg)
}

const USAGE: &str = "\
mythos xtask — usage: cargo xtask [command] [args]

Run with no command for an interactive menu, or call a command directly:

  test [filter]      run the test suite (optionally filtered by name)

  perft              fast perft suite (~17M nodes, the correctness gate)
  perft-deep         thorough perft suite (~800M nodes)
  perft-bench [--tt] [fen] [depth]
                     time a perft and report nodes / elapsed / NPS
                     (start position at depth 6 by default)
  bench-suite [--tt] Andrew Wagner's verified suite (127 positions, ~4.7B
                     nodes) via the engine's `perftsuite` command
  divide [fen] [depth]
                     per-move node counts via UCI `go perft`, to bisect a
                     perft mismatch (start position at depth 1 by default)
  bench              make/unmake micro-benchmark (100M pairs)
  search-bench [depth] [--hash MB]
                     run the search to a fixed depth over 22 suite positions
                     and report the node count — a functional fingerprint of
                     the search (depth 13, hash 16 MB by default). At the
                     default hash the table sits near-empty, so replacement
                     policy is invisible; pass a small --hash to saturate it
  vs-search-bench [ref] [depth] [--hash MB]
                     search-bench the working tree vs a git ref (default
                     HEAD) and diff per-position node counts and best moves.
                     --hash is passed to both binaries, so the base ref must
                     be new enough to understand the flag

  release [--nopext]
                     build the shippable binary matrix (x86-64 v1/v2/v3 for
                     Linux and Windows) into target/release-artifacts/, then
                     verify every binary reports the Cargo.toml version, that
                     all baselines agree on the bench node count, and that the
                     .exe files pull in no DLLs beyond the Windows defaults.
                     --nopext adds a v3 built without BMI2, whose PEXT movegen
                     is very slow on Zen 1/2

  sprt [--ref REF] [--elo0 E] [--elo1 E] [--tc TC]
       [--concurrency N] [--rounds N] [--book PATH] [--affinity CPUS]
                     SPRT match of the working tree vs a git ref (default
                     HEAD) via fastchess; defaults: elo bounds [0, 5],
                     tc 8+0.08, half the cores, 20000-round cap; writes an
                     annotated report.md into the run folder.
                     --affinity pins engines to cores: a CPU list like
                     0-7 or 3,5,7-11, or `auto` to let fastchess choose
  sprt-report RUN    (re)generate report.md for an SPRT run folder — RUN is
                     a target/sprt/runs/<run> path or just the run name

  help               show this help
";
