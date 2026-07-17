<div align="center">

# Mythos

**A UCI chess engine written in Rust.**

</div>

Mythos is an engine I'm building from scratch to learn rust and coding in general, and end up with a good engine of my own.

It speaks the [UCI protocol](https://backscattering.de/chess/uci/), so it plugs
into any standard chess GUI.

> **Status:** playable but early. Board representation, legal move generation, and
> a basic alpha-beta search all work. Not yet rated.

## Building

You'll need **Rust 1.85 or later**.

```bash
git clone https://github.com/dhg14n9/mythos.git
cd mythos
cargo build --release
```

By default the build targets your local CPU (`target-cpu=native` in
`.cargo/config.toml`), which enables the faster PEXT move-generation path on CPUs
with BMI2. If you want a portable binary to share or run on another machine,
remove or override that flag before building.

## Usage

Mythos is a UCI engine — it has no graphical interface of its own. Point a chess
GUI at the compiled binary and it'll take care of the rest.

### Talking to it directly

You can also drive it by hand over stdin:

```
$ ./target/release/mythos
uci
position startpos moves e2e4 e7e5
go movetime 1000
```


## Development

Dev and test automation lives in the `xtask` crate. Run it with no arguments for
an interactive menu, or call a command directly:

```bash
cargo xtask              # interactive menu
cargo xtask <command>    # run a command directly
```

| Command | What it does |
|---|---|
| `test [filter]` | run the test suite |
| `perft` | fast perft suite (~17M nodes) |
| `perft-deep` | thorough perft suite (~800M nodes) |
| `perft-bench [--tt] [fen] [depth]` | time a perft and report nodes / elapsed / NPS |
| `bench-suite [--tt]` | Andrew Wagner's verified suite (127 positions, ~4.7B nodes) |
| `divide [fen] [depth]` | per-move node counts via UCI `go perft`, to bisect a perft mismatch |
| `bench` | make/unmake micro-benchmark |
| `search-bench [depth]` | fixed-depth search over 22 positions, reports the node count (a functional fingerprint of the search) |
| `sprt` | SPRT match of the working tree vs a git ref |

Plain `cargo build` / `cargo test` / `cargo run` are unchanged — xtask only wraps
the workflows that need extra flags or orchestration.

### Strength testing (SPRT)

```bash
cargo xtask sprt [--ref REF] [--elo0 E] [--elo1 E] [--tc TC]
                 [--concurrency N] [--rounds N] [--book PATH]
```

Builds the working tree and a baseline (`--ref`, default `HEAD`), then plays a
[SPRT](https://www.chessprogramming.org/Sequential_Probability_Ratio_Test) match
between them until the elo bounds (default `[0, 5]`) are accepted or rejected.
Defaults: `8+0.08` time control, half the CPU cores, 20000-round cap. Requires
[fastchess](https://github.com/Disservin/fastchess) on `PATH`. Games are saved to
`target/sprt/games.pgn`. Openings come from `xtask/books/openings.epd`, a
500-position sample of `noob_3moves.epd` from the
[official-stockfish books](https://github.com/official-stockfish/books) collection.

## Acknowledgements

Mythos leans heavily on the work and generosity of the computer-chess community:

- **[Reckless](https://github.com/codedeliveryservice/Reckless)** by
  codedeliveryservice — a top-tier open-source Rust engine that I use as a primary
  reference for modern engine architecture.
- The **[Chess Programming Wiki](https://www.chessprogramming.org/)** — the
  indispensable reference for essentially every technique here.
- **[Perft results](https://www.chessprogramming.org/Perft_Results)** and Andrew
  Wagner's [verified perft suite](http://www.rocechess.ch/perft.html) for
  move-generation correctness.
- **[PeSTO](https://www.chessprogramming.org/PeSTO%27s_Evaluation_Function)** by
  Ronald Friederich — the tapered piece-square tables and material values used by
  the current evaluation.


## License

Mythos is released under the [MIT License](LICENSE). I don't really expect anyone to care about this 
but if you do, (Thank you!! :3) just do whatever you want with it honestly. 

---

*Mythos is written and maintained by Do Hoang Giang.*
