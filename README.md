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
  the legacy evaluation.
- **[OpenBench](https://github.com/AndyGrant/OpenBench)** by Andrew Grant — Mythos
  used for sprt, SPSA tuning
- **[bullet](https://github.com/jw1912/bullet)** by jw1912 — NNUE trainer,
  and bulletformat for the training-data format.
- **[Leela Chess Zero](https://lczero.org/)** and everyone who contributes games
  to it — the T91 run's data is what the current net is pretrained on


## License

Mythos is released under the [MIT License](LICENSE). I don't really expect anyone to care about this 
but if you do, (Thank you!! :3) just do whatever you want with it honestly. 

---

*Mythos is written and maintained by Do Hoang Giang.*
