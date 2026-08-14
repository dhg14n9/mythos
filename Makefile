# Build entry point for OpenBench.
#
# OpenBench runs `make EXE=Engine-<sha>` from the repo root and then invokes
# `./Engine-<sha> bench`, so the default goal has to leave a runnable binary at
# exactly $(EXE). `?=` means a command-line EXE= wins while a bare `make` still
# works locally. OpenBench also passes CC=/CXX=; those are for C/C++ engines and
# are harmless here.
#
# Everything below is a thin wrapper around cargo — `cargo build` remains the
# normal way to build, and `cargo xtask` still owns the dev workflows.

EXE ?= mythos

# --bin mythos keeps the workspace's tuner/ and xtask/ members out of the build;
# an OpenBench worker has no reason to compile the dev tooling.
#
# No RUSTFLAGS here: .cargo/config.toml already sets target-cpu=native, which is
# what we want since each client compiles locally. It changes speed but not the
# bench node count (PEXT and the fallback generate identical attack sets), so
# clients on different CPUs still agree on the node count OpenBench verifies.
.PHONY: all
all:
	cargo build --release --bin mythos
	cp target/release/mythos $(EXE)

# Only removes the copied binary. Deliberately not `cargo clean`: target/ also
# holds target/sprt/runs/ and target/vsbench/, which are results you want to
# keep. Use `cargo clean` by hand if you really mean to wipe those too.
.PHONY: clean
clean:
	rm -f $(EXE)
