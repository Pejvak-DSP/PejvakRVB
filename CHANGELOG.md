# Changelog

All notable changes to PEJVAK RVB are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Released]

_No changes yet._

## [1.1.0] - 2026-11-02

### Added

- **Check for Updates** — a new item in the ☰ menu that queries the
  GitHub Releases API, compares the remote tag against `CARGO_PKG_VERSION`,
  and displays the release notes for any newer version. The installer
  for the current platform is selected automatically from the release
  assets (`PEJVAK-RVB-windows-x64-setup.exe`,
  `PEJVAK-RVB-macos-universal.pkg`, `PEJVAK-RVB-linux-x64.tar.gz`).
  Download runs on a background thread with SHA-256 verification as it
  streams. The plugin never executes a downloaded installer without an
  explicit user confirmation checkbox.

- **Bypass pulse** — when the Bypass toggle in the header is engaged,
  its background and label luminance now "breathe" at 1.5 Hz. The
  animation is driven entirely by `request_repaint()` inside the toggle
  handler, so idle CPU cost is unchanged when Bypass is off.

- **Plugin disabled overlay** — when the host takes the plugin offline
  (calls `Plugin::deactivate`), a full-window dimmed overlay appears with
  the text "PLUGIN DISABLED — Enable the plugin in your DAW to resume
  editing." The overlay blocks all mouse interaction with the widgets
  underneath. It disappears as soon as the host calls `Plugin::activate`.

- **ILLUSION `Spark` fade-in** — moving Spark from exactly 0 to any
  positive value now fades the granular cloud in over one second. The
  fade is a one-shot `entrance_gain` ramp, so once it has finished the
  steady-state behaviour is identical to before, and a preset load with
  a non-zero Spark applies instantly.

- **ILLUSION `Rate` maximum raised to 10 Hz** — the clock speed of the
  granular cloud can now reach 10 Hz (previously 8 Hz). The exponential
  mapping, the display formatter, and the text parser are all updated
  to match. Note: since the mapping is exponential, existing values in
  a normalised preset will sound ~11 % faster than before — this is
  unavoidable when widening the top of an exponential range.

- **Releases now ship proper installers** — `release.yml` produces a
  self-extracting `.exe` for Windows (which unblocks MotW and copies
  both formats into `Common Files`), a `.pkg` for macOS (which installs
  into `~/Library/Audio/Plug-Ins/` and removes the quarantine attribute),
  and a `.tar.gz` for Linux. Asset filenames are stable across releases
  so the built-in updater can always find them.

### Changed

- **Texture layer seeds are now per-instance** — Crystal, Scatter, and
  ILLUSION previously seeded their grain RNG with a fixed constant, so
  two instances of the plugin in the same session played the exact same
  random sequence. Seeds now mix in a global instance counter.
  *Subtle audible change:* the granular layers draw a different (but
  statistically identical) random sequence than before.

- **Documentation corrections** — the Velvet saturation stage's
  asymptote is 1.8 (not "limits to 1.0"), the Mist LFO range is
  0.05–25 Hz (not 0.05–2 Hz), and the crate docs list Bloom (not
  Ghost) among the texture layers.

- **README security section** — a new top-level section documents the
  audio-thread safety contract, the Smart Idle behaviour, the updater's
  HTTPS-only / SHA-256 / explicit-confirmation design, and the input
  validation guarantees (XML parsing, parameter string filtering,
  delay clamps).

### Fixed

- **Typed integer values for PreDelay and TapLength were off by one** —
  typing "80" into PreDelay displayed "79 ms", and typing "450" into
  TapLength displayed "449 ms". Root cause: `scaled_parameter_value`
  uses `(p * A) as i64` truncation, and `parse_parameter`'s bisection
  converges to a normalised value whose f32 representation lies just
  below the exact crossing point. The new implementation walks f32 ULPs
  outward from the bisection midpoint until `scaled_parameter_value`
  returns the target integer, which always happens within one or two
  steps. `denormalize` (used for XML loading) uses the same walk, so
  preset round-trips also stay exact.

- **Out-of-bounds read at maximum PreDelay** — at its 1000 ms maximum
  the PreDelay parameter requested exactly `sample_rate` samples,
  equalling the delay buffer length. The two-tap interpolation then
  wrapped a read index out of bounds: release builds silently collapsed
  the delay to zero, debug builds tripped an assertion.
  `ModulatedDelay::update()` now clamps the delay to the usable range
  (`len - 2`), and the pre-delay buffers carry two samples of headroom.
  The same clamp was added to `ModulatedAllpass::update()` (a 100 ms
  diffusion delay at 192 kHz produced exactly this condition), and
  `MultitapDiffuser::update()` now clamps tap positions to the buffer
  length.

- **Sample-rate changes left rate-derived buffers stale** —
  `set_samplerate`/`set_sample_rate` refreshed filter coefficients but
  never reallocated the buffers sized from the construction rate: the
  1 s pre-delay, the multitap buffer, the per-line delay buffers of
  the CloudSeed lines and the FFDN. After a host rate change, large
  delay settings were silently clamped shorter. All of them now
  reallocate on rate change (control path only), and the FFDN re-derives
  its delay layout afterwards.

- **Velvet feedback matrix dropped trailing lines** — `VelvetMatrix`
  truncated the line count to the nearest multiple of its 4-line block,
  so line counts of 5, 6, 7, 9, 10, 11 left the trailing lines' feedback
  inputs unwritten (stale values, lines effectively dropped out of the
  network). A final partial block is now handled as a smaller Householder
  reflection, which stays orthogonal for every line count.

- **Typed "800 ms" decay parsed as 800 seconds** — the LineDecay
  parameter is stored in seconds but displayed in milliseconds below
  one second; the text parser only handled the seconds reading, so
  typing the displayed value saturated at the 60 s maximum. "800 ms"
  now parses to 0.8 s; all genuinely-millisecond parameters are
  unchanged.

- **Texture chain order not persisted** — the layer arrangement of the
  Texture Rack was session-only; reloading a project silently reverted
  to the default order. The packed order is now saved with the plugin
  state (`#[persist = "texture_order"]`).

- **A repeating panic no longer degrades the audio thread
  indefinitely** — the DSP safety net previously swallowed every panic
  and continued on corrupted state. The first caught panic still wipes
  the DSP and re-pushes all parameters; a second strike latches the
  engine to silence until `activate`/`reset` re-arms it.

- **CI pin for `cargo-nice-plug`** — the release workflow now installs
  `cargo-nice-plug` at `--version 0.1.1`. The tool's CLI is not stable
  across versions, and an upstream release could have broken the
  workflow without any local change.

- **`if_same_then_else` clippy errors** — `parse_parameter` merged its
  dB / percent / line-decay branches into single conditions, and
  `scatter_density_parser` merged its two grain-rate branches. Behaviour
  is identical; the merged conditions simply satisfy
  `clippy::if_same_then_else`.

- **`type_complexity` clippy errors in the updater** — the return type
  of `updater::check_inner` was factored into a `CheckResult` type
  alias, and the twenty parser functions in `lib.rs` now share a single
  `Parser` alias. Neither change alters the compiled output; both
  satisfy `clippy::type_complexity`.

### Security

- **Command injection in `manual.rs::open_path` (Windows)** — the
  previous implementation called `cmd /c start "" "<path>"`. `cmd.exe`
  parses its whole command line for metacharacters (`&`, `|`, `^`, `>`,
  `<`, `%`, `!`) *before* the path reaches `start`, so a username or
  install directory containing any of those would have been interpreted
  as a separate command. The function now calls `ShellExecuteW` directly
  via `windows-sys`, which takes a wide string and never goes through
  the shell. The same fix was applied to `updater.rs::open_url`.

- **Updater network requests are HTTPS-only** — `ureq` is configured
  through `AgentBuilder::https_only(true)`. ureq's default
  (`https_only = false`) would silently follow an HTTPS→HTTP redirect
  issued by the server or an on-path attacker. With `https_only(true)`
  the request fails instead.

- **Updater installers are SHA-256 verified as they download** — the
  `download_installer` function streams the response through a
  `Sha256` hasher, then compares the result against GitHub's immutable
  `digest` field (published by the Releases API at upload time).
  A mismatch deletes the partial file and refuses the install. A
  release with no `digest` field is also rejected — the plugin never
  installs an unverified binary.

- **Updater threads are wrapped in `catch_unwind`** —
  `start_update_check` and `start_update_download` run
  `check_now()` / `download_installer()` inside
  `std::panic::catch_unwind(AssertUnwindSafe(...))`. A panic in the
  networking code now produces a visible error message *and* releases
  the `update_busy` flag, which previously would have stayed locked
  until the plugin was reloaded.

- **Defensive clamps in the DSP `unsafe` paths** —
  `ModulatedAllpass::process_no_mod` clamps `sample_delay` to
  `DELAY_BUFFER_SAMPLES`, `ModulatedDelay::new` clamps
  `delay_buffer_size_samples` to at least 2, and
  `MultitapDiffuser::new` clamps `delay_buffer_size` to at least 1.
  These are all no-ops for every legitimate configuration; they exist
  solely to guarantee that a future parameter-range change cannot
  produce undefined behaviour inside an `unsafe` block.

### Removed

- Dead `mono_scratch` buffer from the plugin wrapper (a per-block
  copy on the mono path that was never read).

### Notes

- **Full DSP bit-exactness verified** — `cargo test -p fdn_reverb_core`
  runs 42 tests, all passing. This includes every `*_is_bit_identical`
  test (`density`, `decorr`, `attack`, `ducking`, `gate`, `mid_focus`,
  `loop_filters`), every `*_keeps_network_stable` test
  (`drive_one`, `dynamics`, `loop_filters`, `changing_line_count`),
  every matrix orthogonality and energy-preservation test, every
  SHA-256 / `ShaRandom` reference test, and every Velvet-Noise
  convolution and density test. The suite was run after every change
  in this release and confirms that no DSP behaviour was altered.

- **Test count** — 53 tests total: 42 DSP tests in `fdn_reverb_core`,
  11 plugin tests in `Pejvak_RVB` (7 for the XML parser and parameter
  handling, plus 4 for the updater's version comparison, digest
  parsing, and error paths).

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

- **Code signing** — the Windows `.exe` and the macOS `.pkg` are not
  yet Authenticode- or Developer-ID-signed, so Windows SmartScreen and
  macOS Gatekeeper will warn users on first run. The SHA-256
  verification in the updater guarantees integrity even without code
  signing; code signing would additionally eliminate the OS warnings.

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
  None; }`). The output is bit-identical — verified by the
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

[Unreleased]: https://github.com/Pejvak-DSP/PejvakRVB/compare/v1.1.0...HEAD
[1.1.0]: https://github.com/Pejvak-DSP/PejvakRVB/compare/v1.0.1...v1.1.0
[1.0.1]: https://github.com/Pejvak-DSP/PejvakRVB/compare/v1.0.0...v1.0.1
[1.0.0]: https://github.com/Pejvak-DSP/PejvakRVB/releases/tag/v1.0.0