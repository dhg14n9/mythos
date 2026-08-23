<div align="center">

# Mythos

**A UCI chess engine written in Rust.**

</div>

Mythos is an engine I'm building from scratch to learn rust and coding in general, and end up with a good engine of my own.

It speaks the [UCI protocol](https://backscattering.de/chess/uci/), so it plugs
into any standard chess GUI.

## Building

You'll need **Rust 1.85 or later**.

```bash
git clone https://github.com/dhg14n9/mythos.git
cd mythos
cargo build --release
```

By default the build targets your local CPU (`target-cpu=native` in
`.cargo/config.toml`), which enables the faster PEXT move-generation path on CPUs
with BMI2.

## Features

### Board & move generation
- [x] Bitboard board representation
- [x] Magic bitboards for sliders, with a PEXT path on BMI2 CPUs
- [x] Fully legal move generation
- [x] Incremental Zobrist hashing 
- [x] Fifty-move and repetition draw detection

### Search
- [x] Fail-soft negamax with alpha-beta
- [x] Iterative deepening
- [x] Aspiration windows with progressive widening
- [x] Principal variation search (zero-window + re-search)
- [x] Transposition table
- [x] Quiescence search
- [x] Null-move pruning
- [x] Reverse futility pruning
- [x] Late move reductions
- [x] Mate scoring, distance-to-mate adjustment
- [x] Late move, futility, SEE, history pruning 

### Move ordering
- [x] TT move first
- [x] Static exchange evaluation, threshold-based
- [x] Good/bad noisy split by SEE
- [x] Killer moves (two per ply)
- [x] Butterfly and Continuation history 
- [x] MVV-LVA

### Evaluation
- [x] Tapered evaluation by game phase
- [x] Material and PeSTO piece-square tables
- [x] Piece mobility (knight, bishop, rook, queen)
- [x] King safety via attacker count and weight
- [x] Pawn structure — passed, isolated, doubled
- [x] Bishop pair and tempo bonuses
- [x] Gradient-descent (Texel) tuner with Adam, memory-mapped datasets,
  K fitting, train/validation split and early stopping
- [ ] NNUE evaluation with incremental accumulator updates

### Tooling & testing
- [x] `cargo xtask` as a single entry point 
- [x] Perft suites
- [x] TT-accelerated perft and `divide` for bisecting move-generation bugs
- [x] Fixed-depth search benchmark as a functional fingerprint of the search
- [x] Automated SPRT against any git ref
- [ ] Self-play data generation for NNUE training


## Thanks

Mythos leans heavily on the work and generosity of the computer-chess community:

- **[Reckless](https://github.com/codedeliveryservice/Reckless)** by codedeliveryservice 
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
