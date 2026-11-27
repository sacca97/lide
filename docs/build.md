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
    fontconfig-devel zlib-devel gcc
```

### Debian / Ubuntu

```sh
sudo apt-get install -y \
    libvulkan1 mesa-vulkan-drivers \
    libwayland-dev libxkbcommon-dev libxkbcommon-x11-0 \
    fontconfig-dev zlib1g-dev build-essential
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

### Cross-compiling for Apple Silicon from Linux

A plain `cargo build --target aarch64-apple-darwin` is not enough. The target
standard library is installable with `rustup`, but several dependencies build C
(notably `blake3`) and the app links Cocoa/Metal frameworks. A native Linux
`cc` rejects Apple flags such as `-arch`; the failure appears in a build script
before the editor's own code is reached.

Use one of these instead:

1. **Build on an Apple Silicon Mac.** This is the supported path:
   ```sh
   rustup target add aarch64-apple-darwin   # only needed when cross-building on Intel
   cargo build --release --target aarch64-apple-darwin
   ```
2. **Cross-build with osxcross and a macOS SDK.** The SDK comes from Xcode
   and must be obtained under Apple's licence. Point both the C compiler used
   by build scripts and Cargo's linker at osxcross:
   ```sh
   export CC_aarch64_apple_darwin=/path/to/osxcross/target/bin/oa64-clang
   export CXX_aarch64_apple_darwin=/path/to/osxcross/target/bin/oa64-clang++
   export CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER=/path/to/osxcross/target/bin/aarch64-apple-darwin-clang
   export MACOSX_DEPLOYMENT_TARGET=11.0
   cargo build --release --target aarch64-apple-darwin
   ```

`cargo-zigbuild` alone is not enough: Zig can emit Mach-O, but the AppKit,
Metal, CoreText and similar frameworks still need the Apple SDK's stubs.

PDFium is optional at build time. On the Mac run:

```sh
./scripts/fetch-pdfium.sh
```

The script selects `pdfium-mac-arm64.tgz`, verifies its pinned SHA-256 and
installs `libpdfium.dylib`. A native build then embeds that library in the
executable, so the result can be distributed on its own.

For a cross build from Linux, the checkout's library belongs to the host, so
name the target library explicitly:

```sh
export LIDE_EMBED_PDFIUM=/path/to/pdfium-mac-arm64/lib/libpdfium.dylib
cargo build --release --target aarch64-apple-darwin
```

Set `LIDE_EMBED_PDFIUM=0` to leave PDFium out of the executable and use the
runtime lookup instead. `.app` bundling, code signing and notarisation are not
set up here.

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
SHA-256 and writes it to `third_party/pdfium/lib/`. A native build then embeds
that library in `lide`, so the executable is self-contained: on first use the
library is written to the user's cache and loaded.

Embedding happens only when the library is present at build time. Set
`LIDE_EMBED_PDFIUM=0` to build without it, or set it to a library path to embed
a different copy (for example, a macOS dylib when cross-compiling). At runtime
`LIDE_PDFIUM_PATH`, `PDFIUM_LIBRARY_PATH`, and a library beside the executable
or in the checkout take precedence over the embedded copy. To use a different
copy without rebuilding:

```sh
export LIDE_PDFIUM_PATH=/path/to/libpdfium.so
```

Without an embedded or runtime PDFium the editor still opens PDFs and reports
their page count; the viewer says that PDFium is missing rather than failing to
open the file.

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
cargo run -p lide -- --snap /tmp/editor.png crates/workspace/src/editor/text/buffer.rs

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

The SyncTeX parser (`third_party/synctex`) is C and is built by `cargo build`
with the system C compiler and zlib, which is why `gcc` and the zlib
development package are in the lists above.
