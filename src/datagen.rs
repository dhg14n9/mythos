use std::fs::{self, File};
use std::io::{self, BufRead, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;

use crate::search::{Search, TimeControl};
use crate::tables::{ThreadData, TransTable};
use crate::types::{Color, Move, MoveList, Rng};
use crate::uci::random_opening;

const USAGE: &str = "\
usage: mythos datagen --out <dir> [options]

  --out <dir>                      write chunks here (created if missing)
  --prefix <name>                  chunk name prefix (default dg)
  --threads <n>                    game threads (default 1)
  --hash <mb>                      hash per thread (default 16)
  --soft-nodes <n>                 stop deepening past this many nodes (default 5000)
  --hard-nodes <n>                 abort a search at this many nodes (default 100000)
  --random-plies <n>               random opening plies, plus 0-1 more (default 8)
  --seed <n>                       base seed (default: from the clock)
  --win-adj <cp> <plies>           adjudicate a win after <plies> plies at |score| >= cp (default 2500 4)
  --draw-adj <from> <cp> <plies>   adjudicate a draw from ply <from> after <plies> plies at |score| <= cp (default 80 10 10)
  --max-plies <n>                  call the game a draw after this many plies (default 400)
  --chunk-games <n>                games per chunk file (default 2000)
  --games <n>                      stop after this many games in total (default 0, unlimited)

One line per game: `<start fen> | <1-0|0-1|1/2-1/2> | <uci>:<score> ...`, scores
from the side to move. A chunk is written as `.games.part` and renamed to `.games`
once whole. `stop` on stdin, or stdin closing, ends every thread after its game.
";

#[derive(Clone)]
struct Config {
    out: PathBuf,
    prefix: String,
    threads: usize,
    hash_mb: usize,
    soft_nodes: u64,
    hard_nodes: u64,
    random_plies: usize,
    seed: u64,
    win_cp: i32,
    win_plies: usize,
    draw_from: usize,
    draw_cp: i32,
    draw_plies: usize,
    max_plies: usize,
    chunk_games: usize,
    games: u64
}

impl Config {
    fn parse(args: &[String]) -> Result<Option<Self>, String> {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        let mut cfg = Config {
            out: PathBuf::new(),
            prefix: "dg".to_string(),
            threads: 1,
            hash_mb: 16,
            soft_nodes: 5000,
            hard_nodes: 100_000,
            random_plies: 8,
            seed,
            win_cp: 2500,
            win_plies: 4,
            draw_from: 80,
            draw_cp: 10,
            draw_plies: 10,
            max_plies: 400,
            chunk_games: 2000,
            games: 0
        };

        let mut args = args.iter();
        while let Some(arg) = args.next() {
            let mut next = |what: &str| -> Result<String, String> {
                args.next().cloned().ok_or(format!("{what} needs a value"))
            };
            fn num<T: std::str::FromStr>(what: &str, raw: String) -> Result<T, String> {
                raw.parse().map_err(|_| format!("{what} expects a number, got `{raw}`"))
            }
            match arg.as_str() {
                "--out" => cfg.out = PathBuf::from(next("--out")?),
                "--prefix" => cfg.prefix = next("--prefix")?,
                "--threads" => cfg.threads = num("--threads", next("--threads")?)?,
                "--hash" => cfg.hash_mb = num("--hash", next("--hash")?)?,
                "--soft-nodes" => cfg.soft_nodes = num("--soft-nodes", next("--soft-nodes")?)?,
                "--hard-nodes" => cfg.hard_nodes = num("--hard-nodes", next("--hard-nodes")?)?,
                "--random-plies" => cfg.random_plies = num("--random-plies", next("--random-plies")?)?,
                "--seed" => cfg.seed = num("--seed", next("--seed")?)?,
                "--win-adj" => {
                    cfg.win_cp = num("--win-adj", next("--win-adj")?)?;
                    cfg.win_plies = num("--win-adj", next("--win-adj")?)?;
                }
                "--draw-adj" => {
                    cfg.draw_from = num("--draw-adj", next("--draw-adj")?)?;
                    cfg.draw_cp = num("--draw-adj", next("--draw-adj")?)?;
                    cfg.draw_plies = num("--draw-adj", next("--draw-adj")?)?;
                }
                "--max-plies" => cfg.max_plies = num("--max-plies", next("--max-plies")?)?,
                "--chunk-games" => cfg.chunk_games = num("--chunk-games", next("--chunk-games")?)?,
                "--games" => cfg.games = num("--games", next("--games")?)?,
                "-h" | "--help" => return Ok(None),
                other => return Err(format!("unknown argument `{other}`")),
            }
        }

        if cfg.out.as_os_str().is_empty() {
            return Err("--out is required".to_string());
        }
        if cfg.threads == 0 || cfg.chunk_games == 0 || cfg.hash_mb == 0 {
            return Err("--threads, --hash and --chunk-games must be at least 1".to_string());
        }
        if cfg.prefix.is_empty() || cfg.prefix.contains(['/', '\\']) {
            return Err("--prefix must be a plain name".to_string());
        }
        Ok(Some(cfg))
    }
}

struct Shared {
    stop: AtomicBool,
    started: AtomicU64,
    games: AtomicU64,
    positions: AtomicU64
}

pub fn run(args: &[String]) -> i32 {
    let cfg = match Config::parse(args) {
        Ok(Some(cfg)) => cfg,
        Ok(None) => {
            print!("{USAGE}");
            return 0;
        }
        Err(e) => {
            eprintln!("datagen: {e}\n\n{USAGE}");
            return 2;
        }
    };
    if let Err(e) = fs::create_dir_all(&cfg.out) {
        eprintln!("datagen: {}: {e}", cfg.out.display());
        return 2;
    }

    let shared = Arc::new(Shared {
        stop: AtomicBool::new(false),
        started: AtomicU64::new(0),
        games: AtomicU64::new(0),
        positions: AtomicU64::new(0)
    });

    {
        let shared = Arc::clone(&shared);
        thread::spawn(move || {
            for line in io::stdin().lock().lines() {
                match line {
                    Ok(l) if matches!(l.trim(), "stop" | "quit") => break,
                    Ok(_) => continue,
                    Err(_) => break,
                }
            }
            shared.stop.store(true, Ordering::Relaxed);
        });
    }

    println!("info datagen threads {} soft-nodes {} hard-nodes {} seed {}",
             cfg.threads, cfg.soft_nodes, cfg.hard_nodes, cfg.seed);

    let mut failed = false;
    thread::scope(|scope| {
        let handles: Vec<_> = (0..cfg.threads)
            .map(|index| {
                let cfg = cfg.clone();
                let shared = Arc::clone(&shared);
                scope.spawn(move || worker(&cfg, &shared, index))
            })
            .collect();
        for handle in handles {
            if let Ok(Err(e)) | Err(e) = handle.join().map_err(|_| io::Error::other("thread panicked")) {
                eprintln!("datagen: {e}");
                failed = true;
            }
        }
    });

    println!("done games {} positions {}",
             shared.games.load(Ordering::Relaxed), shared.positions.load(Ordering::Relaxed));
    if failed { 1 } else { 0 }
}

struct Chunk {
    dir: PathBuf,
    name: String,
    writer: BufWriter<File>,
    games: usize,
    positions: usize
}

impl Chunk {
    fn open(dir: &Path, prefix: &str, thread: usize, seq: usize) -> io::Result<Self> {
        let name = format!("{prefix}-t{thread:02}-{seq:05}.games");
        let file = File::create(dir.join(format!("{name}.part")))?;
        Ok(Self { dir: dir.to_path_buf(), name, writer: BufWriter::with_capacity(1 << 16, file), games: 0, positions: 0 })
    }

    fn close(mut self) -> io::Result<()> {
        let part = self.dir.join(format!("{}.part", self.name));
        if self.games == 0 {
            drop(self.writer);
            return fs::remove_file(part);
        }
        self.writer.flush()?;
        self.writer.get_ref().sync_all()?;
        fs::rename(&part, self.dir.join(&self.name))?;
        println!("chunk {} games {} positions {}", self.name, self.games, self.positions);
        Ok(())
    }
}

fn worker(cfg: &Config, shared: &Shared, index: usize) -> io::Result<()> {
    let mut rng = Rng::new(cfg.seed ^ (index as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15));
    let tt = TransTable::new(cfg.hash_mb);
    let mut td = ThreadData::new();
    let mut seq = 0;
    let mut chunk = Chunk::open(&cfg.out, &cfg.prefix, index, seq)?;
    let mut line = String::new();

    while !shared.stop.load(Ordering::Relaxed) {
        if cfg.games > 0 && shared.started.fetch_add(1, Ordering::Relaxed) >= cfg.games {
            break;
        }

        let (positions, next_td) = play_game(cfg, &mut rng, &tt, td, &mut line);
        td = next_td;
        if positions == 0 {
            continue;
        }

        chunk.writer.write_all(line.as_bytes())?;
        chunk.games += 1;
        chunk.positions += positions;
        shared.games.fetch_add(1, Ordering::Relaxed);
        shared.positions.fetch_add(positions as u64, Ordering::Relaxed);

        if chunk.games >= cfg.chunk_games {
            chunk.close()?;
            seq += 1;
            chunk = Chunk::open(&cfg.out, &cfg.prefix, index, seq)?;
        }
    }

    chunk.close()
}

fn play_game(cfg: &Config, rng: &mut Rng, tt: &TransTable, mut td: ThreadData, line: &mut String) -> (usize, ThreadData) {
    use std::fmt::Write as _;

    line.clear();

    let opening;
    (opening, td) = random_opening(rng, tt, td, cfg.random_plies);
    let Some(mut board) = opening else {
        return (0, td);
    };
    tt.clear();
    td.clear();

    let start_fen = board.to_fen();
    let mut moves: Vec<(Move, i32)> = Vec::with_capacity(256);
    let mut win_streak = 0usize;
    let mut win_sign = 0;
    let mut draw_streak = 0usize;
    let start_ply = fen_ply(&start_fen);
    let mut list = MoveList::new();

    let white_result = loop {
        list.clear();
        board.gen_move(&mut list, false);
        if list.len() == 0 {
            break if board.is_check() { if board.stm() == Color::White { -1 } else { 1 } } else { 0 };
        }
        if board.is_draw() || moves.len() >= cfg.max_plies {
            break 0;
        }

        let mut tc = TimeControl::infinite();
        tc.soft_node = cfg.soft_nodes;
        tc.hard_node = cfg.hard_nodes;
        let mut search = Search::new(tc, tt.clone(), td);
        search.silent = true;
        let (mv, score) = search.iterative(&mut board, 100);
        td = search.thread_data;

        if mv.is_null() {
            break 0;
        }
        moves.push((mv, score));
        let white_score = if board.stm() == Color::White { score } else { -score };
        let ply = start_ply + moves.len() - 1;

        if score.abs() >= cfg.win_cp && white_score.signum() == win_sign {
            win_streak += 1;
        } else if score.abs() >= cfg.win_cp {
            win_sign = white_score.signum();
            win_streak = 1;
        } else {
            win_streak = 0;
        }
        if ply >= cfg.draw_from && score.abs() <= cfg.draw_cp {
            draw_streak += 1;
        } else {
            draw_streak = 0;
        }

        board.make_move(mv);

        if win_streak >= cfg.win_plies {
            break win_sign;
        }
        if draw_streak >= cfg.draw_plies {
            break 0;
        }
    };

    if moves.is_empty() {
        return (0, td);
    }

    let result = match white_result {
        1 => "1-0",
        -1 => "0-1",
        _ => "1/2-1/2",
    };
    let _ = write!(line, "{start_fen} | {result} |");
    for (mv, score) in &moves {
        let _ = write!(line, " {mv}:{score}");
    }
    line.push('\n');

    (moves.len(), td)
}

fn fen_ply(fen: &str) -> usize {
    let mut fields = fen.split_whitespace();
    let black = fields.nth(1) == Some("b");
    let fullmove: usize = fields.nth(3).and_then(|f| f.parse().ok()).unwrap_or(1);
    2 * fullmove.saturating_sub(1) + black as usize
}
