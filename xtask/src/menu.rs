use inquire::{Confirm, InquireError, Select, Text};

use crate::release;
use crate::tasks;
use crate::util::Result;
use crate::vs_bench;

const ITEMS: &[&str] = &[
    "test — run the test suite (optional filter)",
    "perft — fast perft suite (correctness gate)",
    "perft-deep — thorough perft suite",
    "bench — make/unmake micro-benchmark",
    "search-bench — fixed-depth search node-count fingerprint",
    "vs-search-bench — diff search-bench of working tree vs a git ref",
    "release — build + verify the shippable binary matrix",
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
            "bench" => tasks::bench(),
            "search-bench" => prompt_search_bench(),
            "vs-search-bench" => prompt_vs_search_bench(),
            "release" => prompt_release(),
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

fn prompt_test() -> Result<()> {
    let filter = ask(Text::new("filter (blank = all)").with_default(""))?;
    tasks::test(Some(&filter))
}

fn prompt_search_bench() -> Result<()> {
    let depth = ask(Text::new("depth").with_default("7"))?;
    let hash = ask(Text::new("hash (MB)").with_default("16"))?;
    tasks::search_bench(Some(&depth), Some(&hash))
}

fn prompt_vs_search_bench() -> Result<()> {
    let gitref = ask(Text::new("base ref").with_default("HEAD"))?;
    let depth = ask(Text::new("depth").with_default("7"))?;
    let hash = ask(Text::new("hash (MB)").with_default("16"))?;
    vs_bench::vs_search_bench(&gitref, &depth, Some(&hash))
}

fn prompt_release() -> Result<()> {
    let nopext = ask_confirm(
        Confirm::new("also build a v3 without BMI2 (for Zen 1/2)?").with_default(false),
    )?;
    release::release(nopext)
}
