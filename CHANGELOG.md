# Changelog

All notable changes to PEJVAK RVB are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Released]

## [1.0.1] - 2026-10-20

### Fixed

- **Zipper noise on manual fader movements** — the Dry, PreDly, Early,
  Late, and Width faders now use per-sample one-pole smoothing (10 ms
  time constant) inside the DSP core. Dragging a fader quickly with the
  mouse no longer produces the staircase amplitude modulation that the
  ear hears as zipper noise. Implemented in `ReverbChannel`
  (CloudSeed) and `ReverbController` (both engines, for Width).
  Uses a NaN-snap mechanism so that preset loads still take effect
  instantly (no 10 ms glide).

- **Zipper noise on host automation** — `SAMPLE_ACCURATE_AUTOMATION`
  is disabled. Previously, nice-plug split the host buffer at every
  automation event, producing a vertical step in the parameter value
  at each split point. This was the actual source of the zipper noise
  heard on automated faders. Setting it to `false` processes full host
  buffers, so `value()` is constant within a buffer and no step can
  occur. Re-enabling it would require `.with_smoother()` on every
  DSP-facing `FloatParam`.

- **FFDN delay buffer sizing** — `SingleNetwork::new` now uses
  `max_delay_samples = ceil(sample_rate × 0.5)` (500 ms at any rate)
  instead of the hard-coded `24000`. At 96 kHz, the old buffer capped
  the usable Size range at 250 ms.

- **FFDN feedback computed from clamped delay** — `recompute_delays`
  now clamps the computed delay to `max_delay_samples()` *before*
  storing it in `delays_samples`. Previously the raw value was stored,
  so the feedback gain was calculated for a delay longer than the line
  could hold — the audible decay came out shorter than requested.

- **Velvet-Noise filter is now sample-rate aware** —
  `VelvetNoiseFilter` is constructed with
  `with_seed_and_rate(seed, sample_rate)`, which scales the pulse
  count, the LP coefficient (true 3.5 kHz at any rate), and the
  maximum RRS width. `ReverbChannel::set_samplerate` calls
  `set_sample_rate()` on both VN filters so host rate changes rebuild
  the internal sequences.

- **FFDN drive stability** — `velvet_saturate` no longer applies a
  makeup term. The previous `1 + 0.15·amount²` makeup gave the shaper
  a small-signal gain of 1.15 at `amount = 1`. Inside the feedback
  loop, any line with `g_mid · 1.15 > 1` would grow instead of decay
  (e.g. `decay = 3 s` and `size ≈ 50 ms` gives `g_mid ≈ 0.89`).
  Removing the makeup restores unity small-signal gain; mid-level
  compression is exactly what a saturation stage should do.

- **FFDN Dynamics audibility** — the three stages were nearly
  inaudible at any setting. They are now tuned so the user can hear
  them:
  - **Attack** uses a square-root time curve and a blend-based
    attenuation (`1 - amount × (1 - ratio)`) so `amount = 0.5`
    produces a clearly audible swell.
  - **Ducking** uses a dB-mapped exponential (`-24 × amount ×
    env_norm`) instead of the linear `1 - amount × env` formula (which
    gave at most ~-3 dB and was imperceptible).
  - **Gate** threshold range is widened to `[-60, -10] dBFS` (was
    `[-80, -30] dBFS`) and the fade is squared. The old maximum was
    below typical FFDN wet levels, so a gate at full amount never
    actually closed.

- **Undo/redo no longer cuts the reverb tail** — `apply_undo_snapshot`
  intentionally does not set `reset_requested`. Undo/redo is a context
  switch between parameter states, not a preset load — clearing the
  reverb tail on every undo was jarring.

- **Preset library canonicalization** — `apply_factory_preset` now
  resets the texture non-mix parameters, Velvet, the seven FFDN card
  master switches, and the EQ master switch *before* applying the
  preset's own parameters. Every factory preset now produces a
  complete, canonical state independent of what the user was doing
  before.

- **Partial XML preset support — CloudSeed** — `ParsedPreset.cloudseed`
  is now `[Option<f32>; 49]`. `xml_to_params` no longer requires all 49
  CloudSeed parameters to be present; a file with only FFDN, only
  Illusion, or a subset of CloudSeed loads non-destructively. A
  completely empty `<CloudReverb></CloudReverb>` is still rejected by
  the `any_param_seen` check. Loading from a file uses
  `apply_cloudseed_partial`, not `apply_preset`.

- **Partial XML preset support — Texture Rack (Known Issue #12)** —
  `ParsedPreset.texture` is now `[Option<f32>; 32]` and
  `ParsedPreset.enabled` is `[Option<bool>; 6]`. `apply_texture_params`
  is now a **partial** apply function (same pattern as
  `apply_cloudseed_partial`, `apply_ffdn_params`, and
  `apply_illusion_params`). Previously, loading a user XML that did
  not contain a `<TextureRack>` block would silently reset every
  texture parameter to 0 and disable every layer, because
  `apply_texture_params` was called unconditionally with all-zero
  arrays. Partial and full XML presets now load non-destructively:
  unspecified texture slots are left untouched, specified slots are
  overwritten. `apply_undo_snapshot` wraps its complete snapshot
  arrays in `Some(..)` and reuses the same partial apply function, so
  there is exactly one texture-apply code path in the codebase.

- **CI race condition** — the three per-platform workflows
  (`build-windows.yml`, `build-macos.yml`, `build-linux.yml`) were
  unified into a single `.github/workflows/release.yml`. The old setup
  had all three workflows racing to create the GitHub Release
  simultaneously, which could result in missing assets or a failed
  release. The new workflow builds all three platforms in parallel,
  uploads artifacts, and creates exactly one release after all builds
  succeed.

### Changed

- **User preset directory scan is throttled** — `user_preset_names()`
  and `user_presets_dir()` now run at most every 0.5 seconds instead of
  every frame. On slow disks with many presets this removes a visible
  UI stutter.

- **`current_preset` is persisted** — the preset name is now stored via
  `#[persist = "current_preset"]`, so closing and reopening the editor
  (or reloading a host project) shows the correct preset label instead
  of always reverting to `Init`.

- **Clippy `-D warnings` clean** — all 26 production lints and all 23
  UI/test lints are now resolved. `cargo clippy --workspace
  --all-targets -- -D warnings` completes with zero warnings. The
  fixes are purely idiomatic (iterator loops instead of `for i in
  0..n`, `is_some_and` instead of `map_or(false, …)`, `clamp` instead
  of `.min().max()`, `rotate_right` instead of a manual bit rotation,
  `as_chunks::<N>()` instead of `chunks_exact(N)` for constant chunk
  sizes, `?` operator instead of `let Some(..) = … else { return
  None; }`). The output is bit-identical — verified by the 33-test
  `cargo test -p fdn_reverb_core` suite, including every
  `*_is_bit_identical` test. One lint (`large_enum_variant` on
  `MatrixKind`) is explicitly allowed at the enum's definition site,
  because boxing the `VelvetMatrix` variant would re-introduce a heap
  allocation on the audio thread in `rebuild_matrix`, violating the
  project's real-time safety guarantees.

- **Redundant permutation loop removed in `velvet_matrix.rs`** —
  `VelvetMatrix::apply` no longer runs the permutation twice. The
  previous code had two identical `for` loops writing the same values
  into `y`; only the `zip`-based version remains. The output was
  already bit-identical (the second loop overwrote the first with the
  same values), so this is a code-clarity change, not a behavioral
  one.

- **CI workflow unified** — see the *CI race condition* entry under
  *Fixed*. The three per-platform workflow files no longer exist.

- **About modal text** — now says "100 factory presets" and "wide 1.6"
  (both were stale).

- **Header order documented** — the actual left-to-right order is:
  `MAIN|TEXTURE → TX → CLOUD|FFDN → Bypass/Freeze/Interp →
  Undo/Redo → Preset → ☰ → ☀/☾`.

- **Repository cleanup** — removed `repomix-instruction.md`,
  `repomix-output.xml`, and `repomix.config.json` from the repository
  and added them to `.gitignore`. These are AI-context files that are
  regenerable and should not be tracked in version control.

- **`CONTRIBUTING.md` refresh** — documented the `MatrixKind`
  inline-storage rule (why clippy's `large_enum_variant` suggestion is
  rejected), the iterator-vs-index-loop convention for DSP hot paths,
  and the partial-texture-XML contract in the PR checklist. Removed
  the Known Issue #12 entry now that it is fixed.

### Notes

- **Full DSP bit-exactness verified** — `cargo test -p
  fdn_reverb_core` now runs 33 tests, all passing. This includes every
  `*_is_bit_identical` test (`density`, `decorr`, `attack`, `ducking`,
  `gate`, `mid_focus`, `loop_filters`), every `*_keeps_network_stable`
  test (`drive_one`, `dynamics`, `loop_filters`, `changing_line_count`),
  every matrix orthogonality and energy-preservation test, every
  SHA-256 / `ShaRandom` reference test, and every Velvet-Noise
  convolution and density test. The suite was run after every change
  in the clippy cleanup and confirms that no DSP behavior was altered.

- **Logo quality in the About modal** — `manual/logo.png` is currently
  low-resolution (~150×200 px) and is upscaled to `64.0 × scale`
  logical pixels in the About modal, producing a pixelated look. This
  is a **file** issue, not a code issue — replacing the file with a
  1024×1024 transparent PNG fixes it with zero code changes.

- **Dynamics parameter tuning in factory presets** — because the
  Attack, Ducking, and Gate stages are now significantly more audible
  at the same parameter values, some factory presets (e.g. *FFDN Soft
  Attack Hall* at `attack = 0.55`) may feel stronger than before. If
  a preset feels too aggressive after this update, lower its
  `attack`/`ducking`/`gate` values in `presets.rs`.

## [1.0.0] - 2025-10-18

Initial public release.

### Added

- CloudSeed legacy-v1 port (49 parameters, pure `f64`).
- FFDN engine with Householder and Velvet matrices.
- ILLUSION granular cloud with pitched overtones.
- Six-layer Texture Rack with reorderable signal chain.
- Velvet-Noise VFDN diffusion inside CloudSeed.
- 100 factory presets across 16 categories.
- Undo/Redo with engine and page restore.
- TX kill switch, Smart Idle, Smart Sleep.

[Unreleased]: https://github.com/Pejvak-DSP/PejvakRVB/compare/v1.0.1...HEAD
[1.0.1]: https://github.com/Pejvak-DSP/PejvakRVB/compare/v1.0.0...v1.0.1
[1.0.0]: https://github.com/Pejvak-DSP/PejvakRVB/releases/tag/v1.0.0
