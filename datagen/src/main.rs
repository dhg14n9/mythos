// PGN -> NNUE training data.
//
// Reads OpenBench DATAGEN PGNs on stdin and writes bulletformat's chess text
// format on stdout: one `<FEN> | <score> | <result>` line per training position.
//
//     bzcat 8.15.*.pgn.bz2 | datagen --out data.txt
//
// Turn that into the binary bullet trains on with bulletformat's own converter,
// which is the reason nothing here packs bytes:
//
//     bulletformat::convert_from_text::<ChessBoard>("data.txt", "data.bin")
//
// Exits non-zero if any game failed to replay — a drop means the converter did
// not understand the input, which is never something to discover later.

mod convert;
mod pgn;
mod san;

use std::fs::File;
use std::io::{self, BufReader, BufWriter, Write};
use std::process::ExitCode;

use convert::{Filters, Stats};

const USAGE: &str = "\
usage: datagen [options] < games.pgn

  -o, --out <path>      write positions here instead of stdout
      --min-ply <n>     skip the first n plies of each game (default 0)
      --max-score <cp>  skip positions scored beyond +/-cp (default 2000)
  -h, --help            show this

Positions are always skipped when the score is a mate score, the comment carries
no score, the played move is a capture or promotion, or the side to move is in
check. Progress and the filter breakdown go to stderr.
";

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("datagen: {e}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<ExitCode, String> {
    let mut out_path: Option<String> = None;
    let mut filters = Filters { min_ply: 0, max_score: 2000 };

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut next = |what: &str| args.next().ok_or(format!("{what} needs a value"));
        match arg.as_str() {
            "-o" | "--out" => out_path = Some(next("--out")?),
            "--min-ply" => {
                filters.min_ply = next("--min-ply")?
                    .parse()
                    .map_err(|_| "--min-ply expects a number".to_string())?
            }
            "--max-score" => {
                filters.max_score = next("--max-score")?
                    .parse()
                    .map_err(|_| "--max-score expects a number".to_string())?
            }
            "-h" | "--help" => {
                print!("{USAGE}");
                return Ok(ExitCode::SUCCESS);
            }
            other => return Err(format!("unknown argument `{other}`\n\n{USAGE}")),
        }
    }

    let reader = BufReader::with_capacity(1 << 20, io::stdin().lock());
    let mut out: BufWriter<Box<dyn Write>> = BufWriter::with_capacity(
        1 << 20,
        match &out_path {
            Some(path) => Box::new(File::create(path).map_err(|e| format!("{path}: {e}"))?),
            None => Box::new(io::stdout().lock()),
        },
    );

    let mut stats = Stats::default();
    let mut buf = String::new();

    for game in pgn::games(reader) {
        let game = game.map_err(|e| format!("reading stdin: {e}"))?;
        stats.games += 1;

        match convert::convert_game(&game, &filters, &mut buf) {
            Ok(counts) => {
                stats.merge_game(&counts);
                out.write_all(buf.as_bytes())
                    .map_err(|e| format!("writing output: {e}"))?;
            }
            Err(reason) => {
                stats.dropped += 1;
                // Enough to diagnose a systematic failure, not enough to bury the
                // summary if the input is wholly wrong.
                if stats.dropped <= 20 {
                    eprintln!("dropped game {}: {reason}", stats.games);
                }
            }
        }

        if stats.games % 10_000 == 0 {
            eprint!("\r{} games, {} positions", stats.games, stats.counts.emitted);
        }
    }

    out.flush().map_err(|e| format!("writing output: {e}"))?;
    report(&stats);

    Ok(if stats.dropped == 0 { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}

fn report(stats: &Stats) {
    let c = &stats.counts;
    let pct = |n: u64| {
        if c.positions == 0 { 0.0 } else { 100.0 * n as f64 / c.positions as f64 }
    };

    eprintln!("\rgames        {:>12}", stats.games);
    eprintln!("dropped      {:>12}", stats.dropped);
    eprintln!("positions    {:>12}", c.positions);
    eprintln!("emitted      {:>12}  {:5.2}%", c.emitted, pct(c.emitted));
    eprintln!("  no score   {:>12}  {:5.2}%", c.no_score, pct(c.no_score));
    eprintln!("  mate       {:>12}  {:5.2}%", c.mate, pct(c.mate));
    eprintln!("  |score|    {:>12}  {:5.2}%", c.big_score, pct(c.big_score));
    eprintln!("  early ply  {:>12}  {:5.2}%", c.early, pct(c.early));
    eprintln!("  noisy      {:>12}  {:5.2}%", c.noisy, pct(c.noisy));
    eprintln!("  in check   {:>12}  {:5.2}%", c.in_check, pct(c.in_check));
}
