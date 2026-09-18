EXE ?= mythos
export EVALFILE

ifdef TUNE
FEATURE_LIST += tunables
endif

ifdef DATAGEN
FEATURE_LIST += datagen
endif

comma := ,
space := $() $()
ifneq ($(strip $(FEATURE_LIST)),)
FEATURES := --features $(subst $(space),$(comma),$(strip $(FEATURE_LIST)))
endif

.PHONY: all
all:
	cargo build --release --bin mythos $(FEATURES)
	cp target/release/mythos $(EXE)

.PHONY: clean
clean:
	rm -f $(EXE)
