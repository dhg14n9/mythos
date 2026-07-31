use std::fmt::Write as _;
use std::path::Path;

use mythos::eval::trace::{initial_weights, BISHOP_MOB, BISHOP_PAIR, DOUBLED_PAWN, ISOLATED_PAWN,
    KNIGHT_MOB, NUM_PARAMS, PASSED_PAWN, PSQT, QUEEN_MOB, ROOK_MOB, TEMPO};

use crate::report::{self, is_dead, param_name, Run};
use crate::train::Weights;
pub type Params = [(i32, i32); NUM_PARAMS];

const CHECKPOINT: &str = "tuned.params";

const PIECE_SQUARE: &str = "src/eval/piece_square.rs";
const MOBILITY:     &str = "src/eval/mobility.rs";
const PAWN:         &str = "src/eval/pawn.rs";
const EVAL:         &str = "src/eval/eval.rs";

pub fn save(dir: &str, weights: &Weights, run: &Run) {
    let path = Path::new(dir).join(CHECKPOINT);

    let mut out = String::new();
    writeln!(out, "# Mythos tuned parameters — {}", report::timestamp()).unwrap();
    writeln!(out, "# K = {:.6}, {} epochs, train {:.6}, validation {:.6}",
        run.k, run.epochs, run.last.0, run.last.1).unwrap();
    writeln!(out, "#").unwrap();
    writeln!(out, "# One line per parameter in trace.rs's flat index space: index mg eg.").unwrap();
    writeln!(out, "# Already rounded to i32 with the structurally dead params zeroed, so").unwrap();
    writeln!(out, "# `tuner emit` is pure formatting — no arithmetic happens downstream of here.").unwrap();

    for i in 0..NUM_PARAMS {
        let (mg, eg) = rounded(weights, i);
        writeln!(out, "{i} {mg} {eg}").unwrap();
    }

    std::fs::write(&path, out)
        .unwrap_or_else(|e| panic!("cannot write {}: {e}", path.display()));

    println!("wrote {}", path.display());
}

fn rounded(weights: &Weights, i: usize) -> (i32, i32) {
    if is_dead(i) { return (0, 0) }

    (weights[i][0].round() as i32, weights[i][1].round() as i32)
}

pub fn load(dir: &str) -> Params {
    let path = Path::new(dir).join(CHECKPOINT);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!(
        "cannot read {} ({e}) — run `tuner train` first, it writes the checkpoint",
        path.display()));

    let mut params = [(0i32, 0i32); NUM_PARAMS];
    let mut seen = [false; NUM_PARAMS];

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') { continue }

        let field: Vec<&str> = line.split_whitespace().collect();
        assert_eq!(field.len(), 3, "{}: expected `index mg eg`, got {line:?}", path.display());

        let n = |s: &str| s.parse::<i32>()
            .unwrap_or_else(|e| panic!("{}: {s:?} is not a number ({e})", path.display()));

        let i = n(field[0]) as usize;
        assert!(i < NUM_PARAMS, "{}: index {i} is outside the {NUM_PARAMS}-parameter space",
            path.display());
        assert!(!seen[i], "{}: index {i} appears twice", path.display());

        params[i] = (n(field[1]), n(field[2]));
        seen[i] = true;
    }

    let missing = seen.iter().filter(|s| !**s).count();
    assert_eq!(missing, 0, "{}: {missing} of {NUM_PARAMS} parameters are missing — \
        a truncated checkpoint would silently ship zeros", path.display());

    params
}

pub fn emit(dir: &str, dry_run: bool) {
    let params = load(dir);
    let live = initial_weights();
    let moved = (0..NUM_PARAMS)
        .flat_map(|i| [(i, 0), (i, 1)])
        .filter(|&(i, _)| !is_dead(i))
        .filter(|&(i, half)| {
            let was = if half == 0 { live[i].0 } else { live[i].1 };
            let now = if half == 0 { params[i].0 } else { params[i].1 };
            was != now
        })
        .count();

    println!("{moved} of {} values differ from the weights compiled into this binary",
        NUM_PARAMS * 2);

    let (mg_value, eg_value) = piece_values(&params);

    let mut psqt: Vec<(&str, String)> = vec![
        ("MG_VALUE", inline(&mg_value)),
        ("EG_VALUE", inline(&eg_value)),
    ];
    let names = ["PAWN", "KNIGHT", "BISHOP", "ROOK", "QUEEN", "KING"];
    let tables: Vec<[Vec<i32>; 2]> = (0..6)
        .map(|pt| [0, 1].map(|half| (0..64)
            .map(|sq| {
                let i = PSQT + pt * 64 + sq;
                let base = if half == 0 { mg_value[pt] } else { eg_value[pt] };
                if is_dead(i) { 0 } else { pick(params[i], half) - base }
            })
            .collect::<Vec<i32>>()))
        .collect();

    for (pt, name) in names.iter().enumerate() {
        for (half, tag) in ["MG", "EG"].iter().enumerate() {
            psqt.push((leak(format!("{tag}_{name}")), ints(&tables[pt][half], 8)));
        }
    }

    let table = |base: usize, len: usize| -> Vec<(i32, i32)> {
        (0..len).map(|n| params[base + n]).collect()
    };

    rewrite(PIECE_SQUARE, &psqt, dry_run);
    rewrite(MOBILITY, &[
        ("KNIGHT_MOBILITY", pairs(&table(KNIGHT_MOB, 9), 7)),
        ("BISHOP_SEED",     pairs(&table(BISHOP_MOB, 14), 7)),
        ("ROOK_SEED",       pairs(&table(ROOK_MOB, 15), 7)),
        ("QUEEN_SEED",      pairs(&table(QUEEN_MOB, 28), 7)),
    ], dry_run);
    rewrite(PAWN, &[
        ("PASS_PAWN_BONUS",   pairs(&table(PASSED_PAWN, 8), 8)),
        ("ISO_PAWN_MALUS",    scalar(params[ISOLATED_PAWN])),
        ("DOUBLE_PAWN_MALUS", scalar(params[DOUBLED_PAWN])),
    ], dry_run);
    rewrite(EVAL, &[
        ("TEMPO_BONUS",       scalar(params[TEMPO])),
        ("BISHOP_PAIR_BONUS", scalar(params[BISHOP_PAIR])),
    ], dry_run);

    if dry_run {
        println!("\n(dry run — nothing was written)");
    } else {
        println!("\nrebuild, then `tuner emit --check` to confirm the tables read back \
            as the checkpoint");
    }
}

pub fn check(dir: &str) {
    let params = load(dir);
    let live = initial_weights();

    let mut bad = 0usize;
    for i in 0..NUM_PARAMS {
        if is_dead(i) { continue }

        if (live[i].0, live[i].1) != params[i] {
            if bad < 12 {
                println!("  {:<22} engine S({}, {})  checkpoint S({}, {})",
                    param_name(i), live[i].0, live[i].1, params[i].0, params[i].1);
            }
            bad += 1;
        }
    }

    assert_eq!(bad, 0, "{bad} parameters disagree — the emitted source is not the \
        checkpoint (did you rebuild after `tuner emit`?)");

    println!("ok: all {NUM_PARAMS} parameters in src/eval match {}/{CHECKPOINT}", dir);
}

fn piece_values(params: &Params) -> ([i32; 6], [i32; 6]) {
    let mut mg = [0i32; 6];
    let mut eg = [0i32; 6];

    for pt in 0..5 {
        let live: Vec<usize> = (0..64)
            .map(|sq| PSQT + pt * 64 + sq)
            .filter(|&i| !is_dead(i))
            .collect();

        let mean = |half: usize| -> i32 {
            let sum: i64 = live.iter().map(|&i| pick(params[i], half) as i64).sum();
            (sum as f64 / live.len() as f64).round() as i32
        };

        mg[pt] = mean(0);
        eg[pt] = mean(1);
    }

    (mg, eg)
}

fn pick(param: (i32, i32), half: usize) -> i32 {
    if half == 0 { param.0 } else { param.1 }
}

fn rewrite(path: &str, items: &[(&str, String)], dry_run: bool) {
    let source = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("cannot read {path}: {e} — run from the repo root"));

    let mut out = source.clone();
    for (name, value) in items {
        out = splice(&out, path, name, value);
    }

    if dry_run {
        println!("\n--- {path} ---");
        for (name, value) in items {
            println!("{name} = {value};");
        }
        return;
    }

    if out == source {
        println!("{path}: unchanged");
        return;
    }

    std::fs::write(path, out).unwrap_or_else(|e| panic!("cannot write {path}: {e}"));
    println!("{path}: rewrote {} tables", items.len());
}


fn splice(source: &str, path: &str, name: &str, value: &str) -> String {
    let declaration = ["const ", "static "].iter()
        .find_map(|keyword| source.find(&format!("{keyword}{name}:")))
        .unwrap_or_else(|| panic!("{path}: no `const {name}` / `static {name}` declaration"));
    
    let eq = declaration + source[declaration..].find('=')
        .unwrap_or_else(|| panic!("{path}: {name} is declared but never assigned"));
    let end = eq + source[eq..].find(';')
        .unwrap_or_else(|| panic!("{path}: {name}'s initialiser has no `;`"));

    format!("{}= {value}{}", &source[..eq], &source[end..])
}

fn inline(values: &[i32]) -> String {
    let cells: Vec<String> = values.iter().map(i32::to_string).collect();

    format!("[{}]", cells.join(", "))
}

fn ints(values: &[i32], per_row: usize) -> String {
    let cells: Vec<String> = values.iter().map(i32::to_string).collect();
    let width = cells.iter().map(String::len).max().unwrap_or(1).max(3);

    let mut out = String::from("[\n");
    for row in cells.chunks(per_row) {
        out.push_str("   ");
        for cell in row {
            write!(out, " {cell:>width$},").unwrap();
        }
        out.push('\n');
    }
    out.push(']');

    out
}

fn pairs(values: &[(i32, i32)], per_row: usize) -> String {
    let mg: Vec<String> = values.iter().map(|v| v.0.to_string()).collect();
    let eg: Vec<String> = values.iter().map(|v| v.1.to_string()).collect();

    let width = |cells: &[String]| cells.iter().map(String::len).max().unwrap_or(1).max(3);
    let (wm, we) = (width(&mg), width(&eg));

    let cells: Vec<String> = mg.iter().zip(&eg)
        .map(|(mg, eg)| format!("S({mg:>wm$}, {eg:>we$})"))
        .collect();

    let mut out = String::from("[\n");
    for row in cells.chunks(per_row) {
        writeln!(out, "    {},", row.join(", ")).unwrap();
    }
    out.push(']');

    out
}

fn scalar(value: (i32, i32)) -> String {
    format!("S({}, {})", value.0, value.1)
}

fn leak(name: String) -> &'static str {
    Box::leak(name.into_boxed_str())
}