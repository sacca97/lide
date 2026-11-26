# Build, run and install lide.
#
#   make build       debug build
#   make agent       debug build of the remote agent
#   make run         debug build, then open the editor here
#   make release     optimized editor and agent
#   make install     optimized build, then install under $(PREFIX)
#   make install-user  same, into $(HOME)/.local (no root)
#   make test        the whole test suite
#   make check       tests plus clippy with warnings denied
#   make snapshots   render every view headlessly
#   make cache-report  show what stale Cargo output could be removed
#   make cache-clean   remove stale Cargo output now
#   make cache-timer   install a weekly systemd user timer for it
#   make where-agent   print the debug agent's path (for scp)
#   make clean       remove the build artifacts
#
# `make install PREFIX=$HOME/.local` installs without root. `DESTDIR` is
# honoured for packaging.

SHELL := /bin/sh

CARGO ?= cargo
# Keep builds out of the way of the desktop. `ionice` is Linux-only.
NICE ?= nice -n 19
IONICE := $(shell command -v ionice >/dev/null 2>&1 && echo "ionice -c 3")
CARGO_LOW := $(IONICE) $(NICE)
# Cap the compiler and the test harness together, so a big workspace does not
# take every core and every queue slot.
JOBS ?= 4
TEST_THREADS ?= 4
# Keep a big build inside a memory-limited user scope, so the desktop cannot
# be pushed into swap or the OOM killer.
MEMORY_MAX ?= 12G
MEMORY_SWAP_MAX ?= 0
SCOPE := $(shell command -v systemd-run >/dev/null 2>&1 && \
	echo "systemd-run --user --scope -p MemoryMax=$(MEMORY_MAX) -p MemorySwapMax=$(MEMORY_SWAP_MAX)")
PREFIX ?= /usr/local
BINDIR ?= $(PREFIX)/bin
LIBDIR ?= $(PREFIX)/lib/lide
DESTDIR ?=
RUN_ARGS ?= .

# The target directory can be redirected in ~/.cargo/config.toml, so ask
# Cargo rather than assuming ./target.
TARGET_DIR := $(shell $(CARGO) metadata --format-version 1 --no-deps 2>/dev/null \
	| python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])' 2>/dev/null)
ifeq ($(strip $(TARGET_DIR)),)
TARGET_DIR := target
endif

DEBUG_BIN := $(TARGET_DIR)/debug/lide
RELEASE_BIN := $(TARGET_DIR)/release/lide
RELEASE_AGENT := $(TARGET_DIR)/release/lide-agent

.PHONY: all build agent where-agent release run install install-user uninstall test check snapshots cache-report cache-clean cache-timer clean

all: build

build:
	$(SCOPE) $(CARGO_LOW) $(CARGO) build -p lide -j $(JOBS)

agent:
	$(SCOPE) $(CARGO_LOW) $(CARGO) build -p lide-remote --bin lide-agent -j $(JOBS)

where-agent: agent
	@printf '%s\n' "$(TARGET_DIR)/debug/lide-agent"

release:
	$(SCOPE) $(CARGO_LOW) $(CARGO) build --release -p lide -p lide-remote -j $(JOBS)

run: build
	"$(DEBUG_BIN)" $(RUN_ARGS)

test:
	$(SCOPE) $(CARGO_LOW) $(CARGO) test --workspace -j $(JOBS) -- --test-threads=$(TEST_THREADS)

check:
	$(SCOPE) $(CARGO_LOW) $(CARGO) test --workspace -j $(JOBS) -- --test-threads=$(TEST_THREADS)
	$(SCOPE) $(CARGO_LOW) $(CARGO) clippy --workspace --all-targets -j $(JOBS) -- -D warnings

snapshots: build
	./scripts/snapshots.sh

# Cargo keeps old build-script directories forever. See the script for what
# it removes; the default is a report, not a deletion.
cache-report:
	./scripts/clean-cargo-cache.sh

cache-clean:
	./scripts/clean-cargo-cache.sh --apply

cache-timer:
	./scripts/clean-cargo-cache.sh --install-timer

install: release
	@if ! install -d "$(DESTDIR)$(BINDIR)" 2>/dev/null; then \
		echo "cannot create $(DESTDIR)$(BINDIR) (permission denied)." >&2; \
		echo "  make install-user                     # into ~/.local, no root" >&2; \
		echo "  sudo make install                    # system-wide" >&2; \
		exit 1; \
	fi
	install -m 0755 "$(RELEASE_BIN)" "$(DESTDIR)$(BINDIR)/lide"
	install -m 0755 "$(RELEASE_AGENT)" "$(DESTDIR)$(BINDIR)/lide-agent"
	@if [ -d third_party/pdfium/lib ] && find third_party/pdfium/lib -type f | grep -q .; then \
		install -d "$(DESTDIR)$(LIBDIR)"; \
		install -m 0644 third_party/pdfium/lib/* "$(DESTDIR)$(LIBDIR)/"; \
		echo "installed PDFium into $(DESTDIR)$(LIBDIR)"; \
	else \
		echo "PDFium not found; run ./scripts/fetch-pdfium.sh for PDF rendering"; \
	fi

# The default PREFIX is the system one; this target keeps `make install`
# usable without root on a personal machine.
install-user: PREFIX := $(HOME)/.local
install-user: install

uninstall:
	rm -f "$(DESTDIR)$(BINDIR)/lide" "$(DESTDIR)$(BINDIR)/lide-agent"
	rm -rf "$(DESTDIR)$(LIBDIR)"

clean:
	$(CARGO_LOW) $(CARGO) clean
