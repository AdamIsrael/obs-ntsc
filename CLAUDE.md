# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

An OBS Studio video filter written in Rust that runs the [ntsc-rs](https://github.com/ntsc-rs/ntsc-rs) NTSC/VHS effect on a source's frames. Registered as an **async video filter** (CPU-side raw frame buffer), so it applies to webcams and media sources but **not** game/display/browser capture.

`ntsc-rs` is pulled as a pinned git dependency, not from crates.io — see the `rev` in `Cargo.toml`.

## Build / install / iterate (macOS)

The local dev loop is driven by `just`:

```sh
just setup-libobs   # one-time: symlink ./vendor/lib/libobs.0.dylib -> OBS framework binary
just build          # cargo build --release with LIBOBS_PATH set
just install        # build + package as .plugin bundle, install into OBS plugins dir
just clean
```

`just install` is the inner-loop command — it rebuilds and replaces the installed plugin every time. Then restart OBS.

To exercise the plugin: add a webcam or media source, right-click → Filters → **+** → "NTSC / VHS Effect". The OBS log dock (View → Logs → View Current Log) is where `log::warn!` / `log::info!` output ends up.

There are no tests; everything is verified by loading into OBS.

## Critical gotchas to know up front

These will save real time:

- **obs-sys 0.2.1 looks for a flat `libobs.0.dylib`** which modern OBS doesn't ship — they use `libobs.framework`. The `setup-libobs` recipe makes a symlink so linking succeeds; at runtime the plugin loads `@rpath/libobs.framework/Versions/A/libobs` just like every other OBS plugin.
- **obs-wrapper's builder never sets `OBS_SOURCE_ASYNC` or `OBS_SOURCE_VIDEO`** when only `filter_video` is wired. We set `OBS_SOURCE_ASYNC_VIDEO` directly on `obs_source_info` in `lib.rs` after `.build()`. Without this the filter ABI is wrong even if it appears to work.
- **`obs_string!` macro requires literal strings**, not `const &str`. When you need to pass a key both as a C string (to OBS) and as a Rust `&str` (to `DataObj::get<T>`), use a literal at the OBS call site and a `const` for the Rust side — they must stay in lockstep.
- **obs-wrapper does not expose property groups, the FILE_SAVE path type, modified callbacks, or `obs_properties_get`.** All of the new UI in `src/properties.rs` is built via raw `obs_sys` FFI. Adding new widgets to the existing layout should stay raw — mixing styles is fragile.
- **`color_matrix_from_video` (in `src/filter.rs`) transmutes `&VideoDataContext` into `*mut obs_source_frame`** assuming `VideoDataContext` is a single-field tuple over that pointer. Verified against obs-wrapper 0.4.1 only. If we ever bump obs-wrapper or it grows a field, this UB-traps silently. Treat this as the most fragile thing in the codebase.

## Architecture

Seven small modules; the interesting flow is the settings ↔ UI roundtrip.

```
src/lib.rs           Module registration. Wires OBS_SOURCE_ASYNC_VIDEO flags.
src/filter.rs        NtscFilter struct, Sourceable/GetDefaults/GetProperties/Update/FilterVideo impls,
                     C callbacks for preset/JSON/export changes, per-frame pipeline dispatch.
src/settings_io.rs   Bridge between obs_data and NtscEffect (descriptor-driven).
src/properties.rs    Raw-FFI property panel builder, descriptor-driven layout.
src/presets.rs       PresetId enum + VHS/Broadcast/Composite NtscEffect builders + intensity scale.
src/colormatrix.rs   Per-frame YUV↔RGB matrix derived from obs_source_frame.color_matrix.
src/yuv.rs           UYVY/NV12 ↔ packed RGBA scratch buffer (uses ColorMatrix).
```

### Settings flow (the core idea)

The properties panel exposes every ntsc-rs parameter as a slider. The dropdown for presets is a *seeding* control: picking a preset writes that preset's full `NtscEffect` into `obs_data` so all sliders snap to those values. The filter then assembles `base_effect` from the sliders on each `read_settings` call. So:

- **Defaults:** `GetDefaultsSource::get_defaults` walks `settings_list().all_descriptors()` and calls `obs_data_set_default_*` for every key, derived from `NtscEffect::default()`.
- **Read (filter ← settings):** `settings_io::read_effect` walks descriptors again, calls `obs_data_get_*` for each, and invokes `(desc.id.set)(&mut effect, AnySetting::*)` to populate a fresh `NtscEffect`.
- **Write (settings ← preset/JSON):** `settings_io::apply_effect_to_settings` uses ntsc-rs's own `to_json_string`, then `obs_data_create_from_json` + `obs_data_apply` to merge into `obs_data`. The JSON keys match ntsc-rs's `SettingDescriptor::id.name`, so exported JSON round-trips with the standalone app for free.
- **Sentinels:** `_last_applied_preset` (int) and `_last_applied_json` (string) live in `obs_data` so the modified callback skips re-apply when the dropdown reopens unchanged. Without these, the user's per-slider tweaks would be wiped every time OBS reopened the properties dialog.

### Per-frame pipeline (`FilterVideoSource::filter_video`)

1. Early-return on zero dimensions or `intensity < 0.01` (pure passthrough, no work).
2. Compute effective effect = `presets::apply_intensity(base_effect, intensity)`.
3. Dispatch on `VideoFormat`:
   - `RGBA` / `BGRA` / `BGRX`: ntsc-rs operates in-place via `YiqOwned::from_strided_buffer` → `apply_effect_to_yiq` → `write_to_strided_buffer`.
   - `UYVY` / `NV12`: pull `ColorMatrix` from raw frame, convert YUV → packed RGBA scratch (reused across frames), run ntsc-rs on RGBA, convert back into the OBS YUV planes.
   - Other formats: log once and pass through.

## Conventions

- **Edition 2024**, GPL-2.0 license (inherited from `obs-wrapper` and `libobs`).
- **Commit style:** short imperative title, body explains *why* (not what). Include `Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>` trailer on Claude-authored commits.
- **Branching:** all work lands on `main`. Tag releases as `v*` to trigger CI.

## CI / releases

`.github/workflows/release.yml` fires on any `v*` tag and builds four/five binaries on native runners:

- `macos-14` → aarch64-apple-darwin
- `macos-15-intel` → x86_64-apple-darwin (note: `macos-13` was deprecated by GH)
- `package-macos-universal` lipos the two macOS builds into a universal Mach-O
- `ubuntu-latest` → Linux x86_64 `.so`
- `windows-latest` → Windows x86_64 `.dll`

Each runner installs OBS via its native channel (brew cask / apt / chocolatey) and uses the same `libobs.0.dylib`-symlink trick (on macOS) or the unversioned `libobs.so` symlink (on Linux) to satisfy the linker. The release job runs `if: always()` so a partial release is still produced if one platform fails.

## Pointers

- README.md — user-facing docs, compatibility table, install paths.
- Both `ntsc-rs` and `obs-wrapper` are vendored into `~/.cargo/registry/src/` after the first build; reading their source (not the docs.rs surface) is usually the fastest way to answer questions about their APIs.
