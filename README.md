# obs-ntsc

An OBS Studio video filter that runs the [ntsc-rs](https://github.com/ntsc-rs/ntsc-rs)
NTSC / VHS effect on a source's frames. Written in Rust as an async video
filter, so it operates on the CPU-side frame buffer of webcams, media files,
and other async video sources.

![Filter applied to a webcam](docs/demo.gif)

## Status

v0.1. GitHub Actions builds binaries for macOS (Apple Silicon, Intel, and
a universal lipo), Linux x86_64, and Windows x86_64 on every `v*` tag,
and attaches them to a draft GitHub Release. Personal testing has been
on macOS Apple Silicon against OBS Studio 32.x; binaries for the other
platforms are produced by CI but not yet user-validated.

## Compatibility

| OBS Studio | Status |
|---|---|
| 32.x | Tested on macOS Apple Silicon |
| 30.x – 31.x | Expected to work; not tested |
| 28.x – 29.x | Likely works — obs-sys 0.2.1's pre-generated bindings target this era |
| &lt; 28 | Not supported |

The plugin links against `libobs`. We haven't observed ABI breaks across the
28 → 32 range, but only OBS 32.x has been user-tested by the author. If you
hit an issue on an earlier version, please open an issue with the OBS log
output.

## Features

- **Three tuned presets** — VHS (worn home-tape look with tracking-noise band,
  edge wobble, head-switching artifacts), Broadcast (clean off-air UHF),
  Composite (RCA-cable / dot-crawl look).
- **Custom presets** — load a `.json` file in the standard ntsc-rs preset
  format, including presets exported from the ntsc-rs standalone app.
- **Intensity slider** — scales the effect from a perfect pass-through at 0
  to the full preset at 1.
- **Color-correct on any source** — uses the per-frame color matrix OBS
  computes for the source, so BT.601, BT.709, and limited- vs full-range
  inputs all roundtrip without color shift.
- **Format coverage** — RGBA, BGRA, BGRX, UYVY, NV12. UYVY and NV12 are
  bridged through a reusable RGBA scratch buffer (most webcams deliver UYVY
  on macOS).

## Requirements

- macOS, Linux, or Windows (CI ships binaries for all of these — see
  *Installation*).
- [OBS Studio](https://obsproject.com/) version 28 or newer recommended.
  On macOS the local justfile install expects OBS at `/Applications/OBS.app`.
- To build from source: Rust toolchain (`rustc`, `cargo`) — edition 2024.
- To use the local install recipe (macOS only):
  [`just`](https://github.com/casey/just).

## Installation

### From GitHub Releases

Each tagged release has prebuilt binaries attached. Download the asset
matching your platform and drop it into your OBS plugins directory:

| Asset | Where it goes |
|---|---|
| `obs-ntsc-<ver>-macos-aarch64.zip` / `-x86_64.zip` / `-universal.zip` | Unzip and place `obs-ntsc.plugin` into `~/Library/Application Support/obs-studio/plugins/` |
| `obs-ntsc-<ver>-linux-x86_64.tar.gz` | Extract `obs-ntsc.so` into `~/.config/obs-studio/plugins/obs-ntsc/bin/64bit/` |
| `obs-ntsc-<ver>-windows-x86_64.zip` | Extract `obs-ntsc.dll` into `%APPDATA%\obs-studio\plugins\obs-ntsc\bin\64bit\` |

### From source (macOS)

```sh
just install
```

That builds the release dylib, packages it as a `.plugin` bundle, and
copies it to `~/Library/Application Support/obs-studio/plugins/obs-ntsc.plugin`.
First run also creates a `vendor/lib/libobs.0.dylib` symlink pointing at
OBS.app's framework binary — see the build notes below.

### Using the plugin

Restart OBS, then on any async video source (webcam, media source):

1. Right-click the source → **Filters**
2. Under **Effect Filters**, click **+**
3. Pick **NTSC / VHS Effect**
4. Choose a preset and adjust intensity

## Usage

The properties panel has these top-level controls:

- **Preset** — VHS / Broadcast / Composite / Custom... Picking a preset
  (re)populates every per-parameter slider in the **NTSC parameters** group
  below. Tweaks you make to those sliders persist; they're only overwritten
  when you change the preset selection.
- **Custom preset JSON** (only when **Custom...** is selected) — pick a
  `.json` file exported from the ntsc-rs standalone app, or any file in the
  same format. Its values are loaded into the sliders.
- **Intensity** (0.0–1.0) — scales magnitude fields and disables tape/noise
  blocks at near-zero values. At 0, the filter passes frames through
  untouched (no YUV roundtrip, no ntsc-rs work).
- **Save preset as...** — pick a path via the native save dialog. The
  current slider state is serialized to that file in the same JSON format
  the standalone app and the Custom-preset loader use, so it round-trips.
  The field clears itself after a successful save.

Below those, an **NTSC parameters** collapsible group exposes every
ntsc-rs parameter directly — chroma lowpass, head switching, tracking
noise, ringing, VHS tape settings, scale, etc. Labels, ranges, and enum
options come straight from ntsc-rs's own setting descriptors.

Bad / missing / unparseable JSON falls back to defaults and logs a
warning to the OBS log dock.

## Known limitations

- **Async sources only.** Game capture, display capture, and browser sources
  use OBS's GPU pipeline — this filter doesn't apply to them. Would require
  a sync (graphics-side) filter variant, deferred.
- **Only macOS Apple Silicon has been user-tested.** GitHub Actions
  produces Linux, Windows, and macOS Intel/universal binaries, but the
  author hasn't loaded them into OBS on those platforms to confirm they
  work end-to-end. Reports welcome.
- **Not codesigned.** macOS will refuse to load it on first launch unless
  you right-click → Open the OBS.app or trust the plugin manually.
- **CPU cost.** UYVY / NV12 sources pay a YUV → RGBA → ntsc-rs → YUV
  roundtrip. Fine at 720p / 1080p on Apple Silicon; not benchmarked at 4K.

## Building from source

```sh
just setup-libobs   # one-time: symlink libobs into ./vendor/lib
just build          # cargo build --release with LIBOBS_PATH set
just install        # build + package as .plugin bundle + install
```

### Why the libobs symlink

The `obs-sys` crate (which `obs-wrapper` depends on) was built against an
older OBS that shipped `libobs.0.dylib` as a flat dylib. Current OBS ships
`libobs.framework` instead. The `setup-libobs` recipe creates a symlink
named `libobs.0.dylib` inside `vendor/lib/` pointing at the framework's
actual binary, so the linker can satisfy the `-lobs.0` directive against
whatever version you have installed.

At runtime the plugin loads against `@rpath/libobs.framework/Versions/A/libobs`
(same as every other OBS plugin), so OBS's own rpath resolves it.

## Acknowledgments

- [ntsc-rs](https://github.com/ntsc-rs/ntsc-rs) by valadaptive — the engine
  doing the actual NTSC simulation.
- [rust-obs-plugins / obs-wrapper](https://github.com/bennetthardwick/rust-obs-plugins)
  by Bennett Hardwick — the Rust binding layer over `libobs`.

## License

GPL-2.0, inherited transitively from `obs-wrapper`. `ntsc-rs` itself is
MIT / ISC / Apache-2.0.
