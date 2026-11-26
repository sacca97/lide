# Building and running

## What you need

* Rust 1.85 or newer (the workspace is developed against 1.99).
* A Vulkan or GLES capable GPU driver, or Mesa's software rasteriser.
* The Wayland and X11 client libraries, for the windowing layer.
* `libxkbcommon` and `libxkbcommon-x11` at runtime.

### Fedora / RHEL

```sh
sudo dnf install -y \
    vulkan-loader vulkan-tools mesa-vulkan-drivers \
    wayland-devel libxkbcommon-devel libxkbcommon-x11 \
    fontconfig-devel
```

### Debian / Ubuntu

```sh
sudo apt-get install -y \
    libvulkan1 mesa-vulkan-drivers \
    libwayland-dev libxkbcommon-dev libxkbcommon-x11-0 \
    fontconfig-dev
```

`libwayland-dev` and `libxkbcommon-dev` are needed at build time because
`wayland-backend`'s `client_system` feature links against them. `libxkbcommon-x11`
is needed at runtime by the X11 backend; without it a windowed run panics at
startup with `Library libxkbcommon-x11.so could not be loaded`.

### macOS (best-effort, not yet verified)

Install the Xcode command-line tools and Rust; there are no X11/Wayland
packages to add:

```sh
xcode-select --install
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Then fetch the PDFium library — the script picks the macOS x64 or arm64 asset
and uses `shasum -a 256` when `sha256sum` is not installed — and build:

```sh
./scripts/fetch-pdfium.sh
cargo build --release
```

The pieces the editor depends on are portable: Rune's Cocoa backend,
`portable-pty`, `dirs` for the standard folders, the system `open` for
"Reveal in file manager", and the external tools on `PATH`. **This repository
has not been built or exercised on macOS**, so treat it as best-effort and
report what breaks.

## Cleaning the build cache

Cargo never removes old build-script output, and a killed build can leave
multi-gigabyte `*.tmp*` linker files in the target directory. The repository
ships a conservative cleaner:

```sh
scripts/clean-cargo-cache.sh                 # report only
scripts/clean-cargo-cache.sh --apply         # remove what it reported
scripts/clean-cargo-cache.sh --days 30 --apply
scripts/clean-cargo-cache.sh --install-timer # weekly systemd user timer
scripts/clean-cargo-cache.sh --uninstall-timer
```

It removes, under the project's target directory, temporary linker files older
than an hour, build-script directories and incremental caches older than
`--days` (default 14), and downloaded crate archives older than the same
period. It does not touch compiled libraries, extracted registry sources, or
any source tree. It takes Cargo's `.cargo-lock`, so it refuses to run while a
build is in progress. The timer runs the same script weekly with `--apply`.

The Makefile exposes the same thing as `make cache-report`, `make cache-clean`
and `make cache-timer`.

## Building

```sh
cargo build --release
```

`make install-user` puts the binary and `lide-agent` in `~/.local/bin` without
root; `sudo make install` installs under `/usr/local`; `PREFIX=...` and
`DESTDIR=...` work for packaging.

The binary is `target/release/lide`. If `~/.cargo/config.toml` sets a
`build.target-dir`, use that directory instead — `cargo metadata
--format-version 1 --no-deps` prints it.

The first build compiles Rune and its dependency tree, which takes a few
minutes. Later builds are incremental.

## PDF rendering (optional)

PDF support needs the PDFium shared library. It is not vendored: fetch it with

```sh
./scripts/fetch-pdfium.sh
```

which downloads the official prebuilt binary, checks it against a pinned
SHA-256 and writes it to `third_party/pdfium/lib/`. The editor finds it there
when it runs from the project directory. To use a different copy:

```sh
export LIDE_PDFIUM_PATH=/path/to/libpdfium.so
```

Without PDFium the editor still opens PDFs and reports their page count; the
viewer says that PDFium is missing rather than failing to open the file.

## Running

```sh
lide                  # an empty workspace
lide .                # the current folder
lide README.md        # one file
lide . README.md      # a folder and a file
```

| Shortcut | Action |
| --- | --- |
| `Ctrl+S` | Save the active document |
| `Ctrl+Shift+S` | Save every document |
| `Ctrl+R` | Reload the active document from disk |
| `Ctrl+W` | Close the active document |
| `Ctrl+Q` | Quit |

Logging goes to standard error. `LIDE_LOG=debug` turns it up.

## Checking the UI without a display

Rune renders the editor headlessly with the same renderer the window uses, so
the views can be checked without a display server:

```sh
# Render the current views to PNGs.
./scripts/snapshots.sh

# Render one view.
cargo run -p lide -- --snap /tmp/editor.png crates/text/src/buffer.rs

# Dump the element tree.
cargo run -p lide -- --tree /tmp/tree.json .
```

`crates/app/tests/snapshots.rs` runs the same renders as a test, so `cargo
test` checks that every view still draws.

## Continuous integration

```sh
cargo test --workspace
cargo build --release
```

The PDF tests need the PDFium library; they skip themselves when it is
missing. The snapshot tests need a GPU or a software rasteriser.

## Notes on the environment

* **Vulkan.** The editor asks for Vulkan first. Mesa's `lavapipe` is a
  software fallback that works headlessly, which is what the snapshot tests
  use on a machine without a GPU.
* **Wayland.** Rune uses `winit` 0.30, which uses `xdg_activation_v1` for
  focus. The editor therefore takes part in the compositor's tiling the way
  any other Wayland client does.
* **X11.** Supported through the same `winit` backend. It needs
  `libxkbcommon-x11` at runtime.

## A note for agents

Do not run the windowed binary on a machine someone is using. Every view can
be rendered and inspected headlessly:

```sh
./scripts/snapshots.sh
cargo test --workspace
cargo run -p lide -- --snap /tmp/out.png <path>
cargo run -p lide -- --tree /tmp/tree.json <path>
```

The windowed binary is for the person at the keyboard.
