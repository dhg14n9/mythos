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
