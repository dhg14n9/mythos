mod menu;
mod release;
mod tasks;
mod util;
mod vs_bench;

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
        "help" | "-h" | "--help" => {
            print!("{USAGE}");
            Ok(())
        }
        other => Err(format!("unknown command: {other}\n\n{USAGE}")),
    }
}

/// `vs-search-bench [ref] [depth]` in either order: a numeric argument is the depth, anything else the git ref.
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

const USAGE: &str = "\
mythos xtask — usage: cargo xtask [command] [args]

Run with no command for an interactive menu, or call a command directly:

  test [filter]      run the test suite (optionally filtered by name)

  perft              fast perft suite (~17M nodes, the correctness gate)
  perft-deep         thorough perft suite (~800M nodes)
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

  help               show this help
";
