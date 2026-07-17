use inquire::{Confirm, InquireError, Select, Text};

use crate::sprt::{self, SprtConfig};
use crate::tasks;
use crate::util::{Result, STARTPOS};

const ITEMS: &[&str] = &[
    "test — run the test suite (optional filter)",
    "perft — fast perft suite (correctness gate)",
    "perft-deep — thorough perft suite",
    "perft-bench — time a perft (fen/depth/tt)",
    "bench-suite — Andrew Wagner 127-position suite",
    "divide — per-move node counts via UCI go perft",
    "bench — make/unmake micro-benchmark",
    "sprt — SPRT match vs a git ref",
    "quit",
];

/// Interactive picker; loops until quit or Esc/Ctrl-C.
pub fn menu() -> Result<()> {
    loop {
        println!();
        let pick = match Select::new("mythos xtask", ITEMS.to_vec()).prompt() {
            Ok(pick) => pick,
            Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => {
                return Ok(());
            }
            Err(e) => return Err(e.to_string()),
        };

        let command = pick.split_whitespace().next().unwrap_or(pick);
        let result = match command {
            "quit" => return Ok(()),
            "test" => prompt_test(),
            "perft" => tasks::perft(),
            "perft-deep" => tasks::perft_deep(),
            "perft-bench" => prompt_perft_bench(),
            "bench-suite" => prompt_bench_suite(),
            "divide" => prompt_divide(),
            "bench" => tasks::bench(),
            "sprt" => prompt_sprt(),
            _ => unreachable!(),
        };

        match result {
            Ok(()) => {}
            // Canceling a parameter prompt just returns to the menu.
            Err(e) if e == CANCELED => {}
            Err(e) => eprintln!("error: {e}"),
        }
    }
}

const CANCELED: &str = "\0canceled";

fn ask(prompt: Text<'_, '_>) -> Result<String> {
    match prompt.prompt() {
        Ok(v) => Ok(v),
        Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => {
            Err(CANCELED.into())
        }
        Err(e) => Err(e.to_string()),
    }
}

fn ask_confirm(prompt: Confirm<'_>) -> Result<bool> {
    match prompt.prompt() {
        Ok(v) => Ok(v),
        Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => {
            Err(CANCELED.into())
        }
        Err(e) => Err(e.to_string()),
    }
}

fn ask_tt() -> Result<bool> {
    ask_confirm(Confirm::new("use transposition table?").with_default(false))
}

fn prompt_test() -> Result<()> {
    let filter = ask(Text::new("filter (blank = all)").with_default(""))?;
    tasks::test(Some(&filter))
}

fn prompt_perft_bench() -> Result<()> {
    let fen = ask(Text::new("FEN").with_default(STARTPOS))?;
    let depth = ask(Text::new("depth").with_default("6"))?;
    let tt = ask_tt()?;
    tasks::perft_bench(tt, Some(&fen), Some(&depth))
}

fn prompt_bench_suite() -> Result<()> {
    tasks::bench_suite(ask_tt()?)
}

fn prompt_divide() -> Result<()> {
    let fen = ask(Text::new("FEN").with_default(STARTPOS))?;
    let depth = ask(Text::new("depth").with_default("1"))?;
    tasks::divide(Some(&fen), Some(&depth))
}

fn prompt_sprt() -> Result<()> {
    let defaults = SprtConfig::default();
    let cfg = SprtConfig {
        gitref: ask(Text::new("baseline ref").with_default(&defaults.gitref))?,
        elo0: ask(Text::new("elo0").with_default(&defaults.elo0))?,
        elo1: ask(Text::new("elo1").with_default(&defaults.elo1))?,
        tc: ask(Text::new("time control").with_default(&defaults.tc))?,
        concurrency: ask(Text::new("concurrency").with_default(&defaults.concurrency))?,
        rounds: ask(Text::new("max rounds").with_default(&defaults.rounds))?,
        book: None,
    };
    sprt::sprt(&cfg)
}
