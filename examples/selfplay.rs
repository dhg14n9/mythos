// Self-play harness: HEAD plays itself over N games at a real time control, and
// dumps every side's history tables plus per-move search statistics as JSON.
//
//   cargo run --release --features stats --example selfplay -- [options]
//
// Normally driven by `cargo xtask selfplay`, which builds this, runs it and turns
// the JSON into the HTML report.
//
// Each side of each game is a separate "engine": its own ThreadData (killers,
// butterfly, continuation) and its own transposition table, kept across the whole
// game and thrown away between games, exactly as a GUI match would.

use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use mythos::board::board::Board;
use mythos::search::{Search, TimeControl};
use mythos::stats::Stats;
use mythos::tables::{ThreadData, TransTable};
use mythos::types::{Color, Move, MoveList, Piece, Square};

const MOVESTOGO: u64 = 40;

struct Config {
    games: usize,
    base_ms: u64,
    inc_ms: u64,
    hash_mb: usize,
    seed: u64,
    top: usize,
    opening_plies: usize,
    max_plies: usize,
    adjudicate_cp: i32,
    adjudicate_plies: usize,
    out: String,
    verbose: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            games: 4,
            base_ms: 8_000,
            inc_ms: 80,
            hash_mb: 16,
            seed: 1,
            top: 20_000,
            opening_plies: 8,
            max_plies: 300,
            adjudicate_cp: 1200,
            adjudicate_plies: 10,
            out: "target/selfplay/run.json".into(),
            verbose: false,
        }
    }
}

// `base+inc` in seconds, e.g. "8+0.08"; a bare "8" means no increment.
fn parse_tc(s: &str) -> Result<(u64, u64), String> {
    let (base, inc) = match s.split_once('+') {
        Some((b, i)) => (b, i),
        None => (s, "0"),
    };
    let secs = |t: &str| -> Result<u64, String> {
        t.trim()
            .parse::<f64>()
            .map_err(|_| format!("bad time control '{s}': expected seconds like 8+0.08"))
            .map(|v| (v * 1000.0).round() as u64)
    };
    let (base, inc) = (secs(base)?, secs(inc)?);
    if base == 0 {
        return Err(format!("bad time control '{s}': base time must be > 0"));
    }
    Ok((base, inc))
}

fn parse_args() -> Result<Config, String> {
    let mut cfg = Config::default();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut it = args.iter();

    while let Some(flag) = it.next() {
        let mut value = || -> Result<String, String> {
            it.next().cloned().ok_or_else(|| format!("{flag} needs a value"))
        };
        let num = |v: &str| -> Result<u64, String> {
            v.parse().map_err(|_| format!("{flag}: '{v}' is not a number"))
        };
        match flag.as_str() {
            "--games" => cfg.games = num(&value()?)? as usize,
            "--tc" => (cfg.base_ms, cfg.inc_ms) = parse_tc(&value()?)?,
            "--hash" => cfg.hash_mb = num(&value()?)? as usize,
            "--seed" => cfg.seed = num(&value()?)?,
            "--top" => cfg.top = num(&value()?)? as usize,
            "--opening" => cfg.opening_plies = num(&value()?)? as usize,
            "--max-plies" => cfg.max_plies = num(&value()?)? as usize,
            "--adjudicate" => cfg.adjudicate_cp = num(&value()?)? as i32,
            "--out" => cfg.out = value()?,
            "--verbose" | "-v" => cfg.verbose = true,
            other => return Err(format!("unknown option: {other}")),
        }
    }
    if cfg.games == 0 {
        return Err("--games must be at least 1".into());
    }
    Ok(cfg)
}

/* ------------------------------------------------------------------ helpers */

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(B64[(n >> 18) as usize & 63] as char);
        out.push(B64[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { B64[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { B64[n as usize & 63] as char } else { '=' });
    }
    out
}

fn legal_moves(board: &Board) -> Vec<Move> {
    let mut list = MoveList::new();
    board.gen_move(&mut list, false);
    (0..list.len()).map(|i| list.get_nth(i)).collect()
}

// a1..h8, one char per square, '.' for empty
fn placement(board: &Board) -> String {
    (0..64).map(|i| board.piece_at(Square::new(i)).to_char()).collect()
}

/* ------------------------------------------------------------------- engine */

struct Engine {
    td: Option<ThreadData>,
    tt: TransTable,
    nodes: u64,
    ms: u64,
    stats: Stats,
}

struct Think {
    mv: Move,
    score: i32,
    nodes: u64,
    ms: u64,
    stats: Stats,
}

impl Engine {
    fn new(hash_mb: usize) -> Self {
        Self {
            td: Some(ThreadData::new()),
            tt: TransTable::new(hash_mb),
            nodes: 0,
            ms: 0,
            stats: Stats::default(),
        }
    }

    fn think(&mut self, board: &Board, hard: Duration, soft: Duration) -> Think {
        let mut b = board.clone();
        let td = self.td.take().unwrap();
        let start = Instant::now();

        let mut search = Search::new(
            TimeControl {
                stop: Arc::new(AtomicBool::new(false)),
                start,
                soft_lim: soft,
                hard_lim: hard,
                soft_base: soft,
            },
            self.tt.clone(),
            td,
        );
        search.silent = true;

        let (mv, score) = search.iterative(&mut b, 100);
        let ms = start.elapsed().as_millis() as u64;

        self.nodes += search.nodes;
        self.ms += ms;
        self.stats.add(&search.stats);
        self.td = Some(search.thread_data);

        Think { mv, score, nodes: search.nodes, ms, stats: search.stats }
    }
}

/* --------------------------------------------------------------------- game */

struct Ply {
    stm: Color,
    mv: Move,
    moved: Piece,
    think: Think,
    clock_ms: u64,
    placement: String,
}

struct Game {
    index: usize,
    seed: u64,
    opening: Vec<Move>,
    start_placement: String,
    result: String,
    reason: String,
    plies: Vec<Ply>,
    white: Engine,
    black: Engine,
}

// Random walk over quiet moves only, so material stays even; rejected unless a
// short scout search agrees the position is roughly balanced.
fn random_opening(cfg: &Config, rng: &mut Rng) -> Option<(Board, Vec<Move>)> {
    let mut board = Board::start_pos();
    let mut moves = Vec::new();

    for _ in 0..cfg.opening_plies {
        let quiets: Vec<Move> = legal_moves(&board).into_iter().filter(|m| m.is_quiet()).collect();
        if quiets.is_empty() {
            return None;
        }
        let mv = quiets[rng.below(quiets.len())];
        board.make_move(mv);
        moves.push(mv);
    }

    if legal_moves(&board).is_empty() || board.is_check() {
        return None;
    }

    let mut scout = Search::new(TimeControl::infinite(), TransTable::new(1), ThreadData::new());
    scout.silent = true;
    let (_, score) = scout.iterative(&mut board.clone(), 10);
    if score.abs() > 100 {
        return None;
    }

    Some((board, moves))
}

fn play_game(cfg: &Config, index: usize) -> Game {
    let seed = cfg.seed.wrapping_mul(0x9E3779B97F4A7C15).wrapping_add(index as u64 + 1);
    let mut rng = Rng(seed);
    let (start_board, opening) = loop {
        if let Some(x) = random_opening(cfg, &mut rng) {
            break x;
        }
    };

    let mut white = Engine::new(cfg.hash_mb);
    let mut black = Engine::new(cfg.hash_mb);
    let mut board = start_board.clone();
    let mut plies: Vec<Ply> = Vec::new();
    let mut clock = [cfg.base_ms as i64, cfg.base_ms as i64];
    let mut streak = 0usize;
    let mut streak_side = Color::White;

    let (result, reason) = loop {
        if legal_moves(&board).is_empty() {
            break if board.is_check() {
                match board.stm() {
                    Color::White => ("0-1", "black mates"),
                    Color::Black => ("1-0", "white mates"),
                }
            } else {
                ("1/2-1/2", "stalemate")
            };
        }
        if board.is_draw() {
            break ("1/2-1/2", "repetition or 50-move");
        }
        if plies.len() >= cfg.max_plies {
            break ("1/2-1/2", "ply limit");
        }

        let stm = board.stm();
        let side = stm as usize;
        let engine = match stm {
            Color::White => &mut white,
            Color::Black => &mut black,
        };

        let (hard, soft) = mythos::uci::limits(clock[side].max(1) as u64, cfg.inc_ms, MOVESTOGO);
        let think = engine.think(&board, hard, soft);

        clock[side] -= think.ms as i64;
        let flagged = clock[side] < 0;
        clock[side] += cfg.inc_ms as i64;

        let moved = board.piece_at(think.mv.from());
        board.make_move(think.mv);

        if cfg.verbose {
            println!(
                "  {:>3}.{} {}{:<6} {:>6}cp  d{:<2} {:>9} nodes {:>6}ms  clock {:>6}ms",
                plies.len() / 2 + 1,
                if stm == Color::White { " " } else { ".." },
                moved.to_char(),
                think.mv.to_string(),
                think.score,
                think.stats.iters.last().map(|i| i.depth).unwrap_or(0),
                think.nodes,
                think.ms,
                clock[side].max(0)
            );
        }

        let score = think.score;
        plies.push(Ply {
            stm,
            mv: think.mv,
            moved,
            think,
            clock_ms: clock[side].max(0) as u64,
            placement: placement(&board),
        });

        if flagged {
            break match stm {
                Color::White => ("0-1", "white forfeits on time"),
                Color::Black => ("1-0", "black forfeits on time"),
            };
        }

        // decisive-score adjudication
        let winner = if score > 0 { stm } else { !stm };
        if score.abs() >= cfg.adjudicate_cp {
            if streak > 0 && streak_side == winner {
                streak += 1;
            } else {
                streak = 1;
                streak_side = winner;
            }
        } else {
            streak = 0;
        }
        if streak >= cfg.adjudicate_plies {
            break match streak_side {
                Color::White => ("1-0", "adjudicated"),
                Color::Black => ("0-1", "adjudicated"),
            };
        }
    };

    Game {
        index,
        seed,
        opening,
        start_placement: placement(&start_board),
        result: result.into(),
        reason: reason.into(),
        plies,
        white,
        black,
    }
}

/* --------------------------------------------------------------------- dump */

/// Butterfly: 4 bytes per nonzero cell — u16 index ((color*64 + from)*64 + to),
/// i16 value (the i32 counter is bounded by MAX_BUTTERFLY, so it fits).
fn pack_butterfly(td: &ThreadData) -> (String, usize, usize) {
    let mut bytes = Vec::new();
    let (mut count, mut capped) = (0usize, 0usize);
    for (c, table) in td.butterfly.raw().iter().enumerate() {
        for (from, row) in table.iter().enumerate() {
            for (to, &v) in row.iter().enumerate() {
                if v == 0 {
                    continue;
                }
                count += 1;
                if v.abs() >= 8192 {
                    capped += 1;
                }
                let idx = ((c * 64 + from) * 64 + to) as u16;
                bytes.extend_from_slice(&idx.to_le_bytes());
                bytes.extend_from_slice(&(v as i16).to_le_bytes());
            }
        }
    }
    (base64(&bytes), count, capped)
}

struct ContDump {
    b64: String,
    total: usize,
    kept: usize,
    capped: usize,
    cutoff: i32,
    ctx_abs: Vec<u64>,
    ctx_cnt: Vec<u32>,
}

/// Continuation: 6 bytes per kept cell — u32 index, i16 value, sorted ascending
/// so the report can range-scan one context. Only the `top` largest cells by
/// magnitude are kept; the per-context aggregates stay exact over the full table.
fn pack_continuation(td: &ThreadData, top: usize) -> ContDump {
    let mut cells: Vec<(u32, i16)> = Vec::new();
    let mut ctx_abs = vec![0u64; 768];
    let mut ctx_cnt = vec![0u32; 768];
    let mut capped = 0usize;

    for (pp, a) in td.continuation.raw().iter().enumerate() {
        for (ps, b) in a.iter().enumerate() {
            let ctx = pp * 64 + ps;
            for (p, c) in b.iter().enumerate() {
                for (to, &v) in c.iter().enumerate() {
                    if v == 0 {
                        continue;
                    }
                    ctx_abs[ctx] += v.unsigned_abs() as u64;
                    ctx_cnt[ctx] += 1;
                    if v.abs() >= 15000 {
                        capped += 1;
                    }
                    cells.push(((ctx * 768 + p * 64 + to) as u32, v));
                }
            }
        }
    }

    let total = cells.len();
    let mut cutoff = 0;
    if top > 0 && cells.len() > top {
        cells.sort_unstable_by_key(|&(_, v)| std::cmp::Reverse(v.unsigned_abs()));
        cutoff = cells[top - 1].1.abs() as i32;
        cells.truncate(top);
        cells.sort_unstable_by_key(|&(idx, _)| idx);
    }

    let mut bytes = Vec::with_capacity(cells.len() * 6);
    for (idx, v) in &cells {
        bytes.extend_from_slice(&idx.to_le_bytes());
        bytes.extend_from_slice(&v.to_le_bytes());
    }

    ContDump {
        b64: base64(&bytes),
        total,
        kept: cells.len(),
        capped,
        cutoff,
        ctx_abs,
        ctx_cnt,
    }
}

/// Joint statistics between the two sides' tables. Computed here, over the full
/// tables, because the report only carries the largest continuation cells — a
/// correlation measured on the truncated dump would be badly inflated.
#[derive(Default)]
struct Pair {
    n: u64,
    same_sign: u64,
    sx: f64,
    sy: f64,
    sxx: f64,
    syy: f64,
    sxy: f64,
}

impl Pair {
    fn push(&mut self, x: f64, y: f64) {
        self.n += 1;
        if (x > 0.0) == (y > 0.0) {
            self.same_sign += 1;
        }
        self.sx += x;
        self.sy += y;
        self.sxx += x * x;
        self.syy += y * y;
        self.sxy += x * y;
    }

    fn r(&self) -> f64 {
        let n = self.n as f64;
        if n < 2.0 {
            return 0.0;
        }
        let cov = self.sxy - self.sx * self.sy / n;
        let vx = self.sxx - self.sx * self.sx / n;
        let vy = self.syy - self.sy * self.sy / n;
        let d = (vx * vy).sqrt();
        if d == 0.0 { 0.0 } else { cov / d }
    }
}

/// Bins per axis of the continuation density map.
const HIST_N: usize = 48;

fn hist_bin(v: i16, max: f64) -> usize {
    let t = (v as f64 + max) / (2.0 * max);
    ((t * HIST_N as f64) as usize).min(HIST_N - 1)
}

fn pair_json(w: &ThreadData, b: &ThreadData) -> String {
    let (mut bf, mut ct) = (Pair::default(), Pair::default());
    let mut hist = vec![0u32; HIST_N * HIST_N];

    let (wb, bb) = (w.butterfly.raw(), b.butterfly.raw());
    for c in 0..2 {
        for from in 0..64 {
            for to in 0..64 {
                let (x, y) = (wb[c][from][to], bb[c][from][to]);
                if x != 0 && y != 0 {
                    bf.push(x as f64, y as f64);
                }
            }
        }
    }

    let (wc, bc) = (w.continuation.raw(), b.continuation.raw());
    for pp in 0..Piece::NUM {
        for ps in 0..Square::NUM {
            for p in 0..Piece::NUM {
                for to in 0..Square::NUM {
                    let (x, y) = (wc[pp][ps][p][to], bc[pp][ps][p][to]);
                    if x != 0 && y != 0 {
                        ct.push(x as f64, y as f64);
                        hist[hist_bin(y, 15000.0) * HIST_N + hist_bin(x, 15000.0)] += 1;
                    }
                }
            }
        }
    }

    format!(
        "{{\"bfBoth\":{},\"bfR\":{:.4},\"bfSame\":{},\"ctBoth\":{},\"ctR\":{:.4},\"ctSame\":{},\
         \"histN\":{HIST_N},\"histMax\":15000,\"hist\":[{}]}}",
        bf.n,
        bf.r(),
        bf.same_sign,
        ct.n,
        ct.r(),
        ct.same_sign,
        hist.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(",")
    )
}

fn stats_json(s: &Stats) -> String {
    format!(
        "{{\"q\":{},\"sd\":{},\"tp\":{},\"th\":{},\"tc\":{},\"co\":{},\"cq\":{},\"ci\":[{}],\"an\":{},\
         \"nt\":{},\"nc\":{},\"rf\":{},\"lm\":{},\"fu\":{},\"se\":{},\"lr\":{},\"lp\":{},\"lre\":{},\"pr\":{}}}",
        s.qnodes, s.seldepth, s.tt_probe, s.tt_hit, s.tt_cut, s.cutoffs, s.cutoff_quiet,
        s.cutoff_idx.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(","),
        s.all_nodes, s.nmp_try, s.nmp_cut, s.rfp_cut, s.lmp_skip, s.futility_skip, s.see_skip,
        s.lmr_reduced, s.lmr_plies, s.lmr_research, s.pv_research
    )
}

fn engine_json(engine: &Engine, top: usize) -> String {
    let td = engine.td.as_ref().unwrap();
    let (bf, bf_count, bf_capped) = pack_butterfly(td);
    let cont = pack_continuation(td, top);

    format!(
        "{{\"nodes\":{},\"ms\":{},\"stats\":{},\
         \"butterfly\":\"{}\",\"bfCount\":{},\"bfCapped\":{},\
         \"cont\":\"{}\",\"contTotal\":{},\"contKept\":{},\"contCapped\":{},\"contCutoff\":{},\
         \"ctxAbs\":[{}],\"ctxCnt\":[{}]}}",
        engine.nodes,
        engine.ms,
        stats_json(&engine.stats),
        bf,
        bf_count,
        bf_capped,
        cont.b64,
        cont.total,
        cont.kept,
        cont.capped,
        cont.cutoff,
        cont.ctx_abs.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(","),
        cont.ctx_cnt.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(","),
    )
}

fn ply_json(p: &Ply) -> String {
    let iters = p
        .think
        .stats
        .iters
        .iter()
        .map(|i| format!("[{},{},{},{},{}]", i.depth, i.score, i.nodes, i.ms, i.researches))
        .collect::<Vec<_>>()
        .join(",");
    let pv = p.think.stats.iters.last().map(|i| i.pv.clone()).unwrap_or_default();

    format!(
        "{{\"stm\":\"{}\",\"uci\":\"{}\",\"from\":{},\"to\":{},\"piece\":{},\"capture\":{},\"quiet\":{},\
         \"score\":{},\"nodes\":{},\"ms\":{},\"clock\":{},\"pv\":\"{}\",\"iters\":[{}],\"stats\":{},\"placement\":\"{}\"}}",
        p.stm.char(),
        p.mv,
        p.mv.from() as usize,
        p.mv.to() as usize,
        p.moved as usize,
        p.mv.is_capture(),
        p.mv.is_quiet(),
        p.think.score,
        p.think.nodes,
        p.think.ms,
        p.clock_ms,
        pv,
        iters,
        stats_json(&p.think.stats),
        p.placement
    )
}

fn game_json(g: &Game, top: usize) -> String {
    format!(
        "{{\"index\":{},\"seed\":{},\"result\":\"{}\",\"reason\":\"{}\",\
         \"opening\":[{}],\"startPlacement\":\"{}\",\
         \"plies\":[{}],\
         \"pair\":{},\
         \"engines\":{{\"white\":{},\"black\":{}}}}}",
        g.index + 1,
        g.seed,
        g.result,
        g.reason,
        g.opening.iter().map(|m| format!("\"{m}\"")).collect::<Vec<_>>().join(","),
        g.start_placement,
        g.plies.iter().map(ply_json).collect::<Vec<_>>().join(",\n"),
        pair_json(g.white.td.as_ref().unwrap(), g.black.td.as_ref().unwrap()),
        engine_json(&g.white, top),
        engine_json(&g.black, top),
    )
}

/* --------------------------------------------------------------------- main */

fn main() {
    let cfg = match parse_args() {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(2);
        }
    };

    if cfg!(not(feature = "stats")) {
        eprintln!(
            "warning: built without the `stats` feature — search counters will all be zero.\n\
             Build with: cargo run --release --features stats --example selfplay"
        );
    }

    let tc = format!("{:.3}+{:.3}", cfg.base_ms as f64 / 1000.0, cfg.inc_ms as f64 / 1000.0);
    println!(
        "self-play: {} game(s), tc {tc}, {} MB hash per engine, seed {}",
        cfg.games, cfg.hash_mb, cfg.seed
    );

    let started = Instant::now();
    let mut games = Vec::with_capacity(cfg.games);
    for i in 0..cfg.games {
        let t0 = Instant::now();
        let game = play_game(&cfg, i);
        println!(
            "  game {}/{}: {} ({}) in {} plies, {:.1}s  [{} + {} nodes]",
            i + 1,
            cfg.games,
            game.result,
            game.reason,
            game.plies.len(),
            t0.elapsed().as_secs_f64(),
            game.white.nodes,
            game.black.nodes
        );
        games.push(game);
    }

    let mut out = String::with_capacity(1 << 22);
    out.push_str("{\n\"config\":{");
    out.push_str(&format!(
        "\"games\":{},\"tc\":\"{tc}\",\"baseMs\":{},\"incMs\":{},\"hashMb\":{},\"seed\":{},\
         \"top\":{},\"openingPlies\":{},\"maxPlies\":{},\"adjudicateCp\":{},\"movestogo\":{},\
         \"stats\":{},\"elapsedSecs\":{:.1}",
        cfg.games, cfg.base_ms, cfg.inc_ms, cfg.hash_mb, cfg.seed, cfg.top, cfg.opening_plies,
        cfg.max_plies, cfg.adjudicate_cp, MOVESTOGO, cfg!(feature = "stats"),
        started.elapsed().as_secs_f64()
    ));
    out.push_str("},\n\"games\":[\n");
    out.push_str(&games.iter().map(|g| game_json(g, cfg.top)).collect::<Vec<_>>().join(",\n"));
    out.push_str("\n]\n}\n");

    if let Some(dir) = std::path::Path::new(&cfg.out).parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let mut f = std::fs::File::create(&cfg.out)
        .unwrap_or_else(|e| panic!("cannot create {}: {e}", cfg.out));
    f.write_all(out.as_bytes()).expect("cannot write output");

    let (w, d, l) = games.iter().fold((0, 0, 0), |(w, d, l), g| match g.result.as_str() {
        "1-0" => (w + 1, d, l),
        "0-1" => (w, d, l + 1),
        _ => (w, d + 1, l),
    });
    println!(
        "\n{} game(s) in {:.1}s — white {w} / draw {d} / black {l}",
        cfg.games,
        started.elapsed().as_secs_f64()
    );
    println!("wrote {} ({:.2} MB)", cfg.out, out.len() as f64 / 1e6);
}
