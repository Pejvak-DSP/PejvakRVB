# Contributing to PEJVAK RVB

First off, thank you for considering contributing to **PEJVAK RVB**! It's people like you who make open-source audio software such a powerful and inspiring community.

This document describes how to contribute, whether you are reporting a bug, suggesting a feature, or submitting a pull request.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Reporting Bugs](#reporting-bugs)
- [Suggesting Features](#suggesting-features)
- [Development Setup](#development-setup)
- [Strict Code Rules (Audio Thread Safety)](#strict-code-rules-audio-thread-safety)
- [DSP-Specific Rules (Non-Negotiable)](#dsp-specific-rules-non-negotiable)
- [Code Style](#code-style)
- [Pull Request Process](#pull-request-process)
- [Testing Checklist for PRs](#testing-checklist-for-prs)
- [Known Issues](#known-issues)
- [License](#license)

---

## Code of Conduct

By participating in this project, you are expected to uphold a welcoming, respectful, and harassment-free environment. We are all here to build great audio tools.

---

## Reporting Bugs

If you find a bug, please open an issue on the [GitHub Issues](https://github.com/Pejvak-DSP/PejvakRVB/issues) page.

To help us fix it quickly, please include:

1. **Environment:** OS version (Windows / macOS / Linux), CPU architecture (x86_64 / Apple Silicon), and DAW name and version.
2. **Plugin version:** The exact version of PEJVAK RVB you are using (visible in the `☰ → About` menu).
3. **Audio settings:** Sample rate (44.1 / 48 / 96 kHz) and buffer size (e.g. 128, 256, 512) at the time the bug occurred.
4. **Steps to reproduce:** A clear, step-by-step description of how to trigger the bug.
5. **Expected vs. actual behavior:** What you thought would happen versus what actually happened.
6. **Screenshots / audio clips:** If applicable, attach a short audio sample demonstrating the issue. This is especially valuable for zipper, click, and tail bugs.

---

## Suggesting Features

We welcome feature requests! Before opening an issue, please check whether a similar request already exists.

When suggesting a feature, explain *why* it would be useful, especially from a DSP, musical, or UI/UX perspective. Concrete examples (e.g. "this would let me do X in my mix") are far more valuable than abstract ideas.

---

## Development Setup

### Prerequisites

- **Rust 1.98+** (install via [rustup](https://rustup.rs))
- **Windows:** Visual Studio Build Tools (C++ workload)
- **macOS:** Xcode Command Line Tools (`xcode-select --install`)
- **Linux:** X11 and OpenGL development libraries:

  ```bash
  sudo apt-get install -y libx11-dev libxcursor-dev libxcb-dri2-0-dev \
    libxcb-icccm4-dev libx11-xcb-dev mesa-common-dev \
    libgl1-mesa-dev libglu1-mesa-dev
  ```

### Building

Build the plugin bundle locally:

```bash
cargo nice-plug bundle Pejvak_RVB --release
```

### Running the DSP tests

```bash
cargo test -p fdn_reverb_core
```

### Linting the whole workspace

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

`--all-targets` is essential: without it, test code is not linted, and violations there go unnoticed until CI catches them. `--workspace` ensures both `fdn_reverb_core` and `Pejvak_RVB` are checked.

---

## Strict Code Rules (Audio Thread Safety)

Audio programming is highly sensitive. A single allocation on the audio thread or a bad lock can cause dropouts or crashes in a DAW. **All pull requests must adhere to the following rules.**

### 1. Real-Time Safety (Zero Allocation)

- **Never** allocate memory (`Vec::new`, `Box::new`, `String::from`, etc.) inside `process()` or any DSP function called from it.
- If you need dynamic memory, pre-allocate it in `new()` or `activate()`.
- Use fixed-size arrays (e.g. `[f64; 16]`) or pre-allocated buffers instead of `Vec` in the audio hot path.
- All `ctrl.set_*` calls (e.g. `set_parameter`, `set_ffdn_*`, `set_texture_parameter`, `set_illusion_*`) are made from inside `process_inner`, i.e. on the audio thread. **They must remain allocation-free.** `MatrixKind` is stored inline precisely so that `rebuild_matrix` can reassign it without touching the heap.

### 2. Pure `f64` Math

- All DSP operations must be done in **`f64`** (double precision).
- `f32` may only be used at the very boundary (plugin input/output buffers).
- **Never** use fast-math or approximate fast-trig functions unless they have been tested and approved for this project.
- **No FMA, no reassociation.** The `wide` SIMD crate is used only for element-wise mul/add on independent data, so that results stay bit-identical to scalar code.

### 3. No Locking on the Audio Thread

- Never use `Mutex`, `RwLock`, or any thread-blocking operation inside `process()`.
- Use atomics (`AtomicBool`, `AtomicU64`) or lock-free structures for communication between the UI and the audio thread.
- On the UI side, when interacting with `egui` memory, avoid nested locks (e.g. calling `ui.ctx().input()` inside `ui.memory_mut()`), as this can deadlock. This is explicitly required by the TX kill switch (`pejvak_tx_kill_active` and `pejvak_force_undo_commit_at`).

### 4. Prevent Denormals

- Recursive filters must flush denormals (values below `1e-30` are treated as `0.0`).
- ILLUSION already does this (`flush_denorm`); any new recursive filter must do the same.

---

## DSP-Specific Rules (Non-Negotiable)

These rules are specific to this project and **must be preserved** by every PR. Each one was learned the hard way, usually after a bug that cost hours of debugging or shipped in a release.

### Zipper Noise: Two Separate Fixes

Zipper noise in this plugin comes from **two independent sources**, and each has its own fix. Do not confuse them.

#### 1. Host automation → `SAMPLE_ACCURATE_AUTOMATION = false`

- `SAMPLE_ACCURATE_AUTOMATION` **must remain `false`** unless every `FloatParam` that feeds the DSP has `.with_smoother()` enabled **and** every path in `process_inner` reads `smoothed.next()` (per sample) or `smoothed.next_block()` (per sub-block).
- With `SAMPLE_ACCURATE_AUTOMATION = true` and un-smoothed parameters, `nice-plug` splits the host buffer at every automation event. This produces a **vertical step** in the parameter value at each split point, heard as zipper noise on automated parameters.
- Setting it to `false` avoids the split entirely: `process_inner` sees a full host buffer, `value()` is constant within the block, and no step can occur.
- Re-enabling it requires the `.with_smoother()` migration described above. Do not flip the flag without that migration.

#### 2. Manual fader movement → per-sample DSP smoothing

Manual fader dragging is a separate problem: even with a full host buffer, each UI frame produces a stepwise parameter change that the DSP would apply as a step in amplitude. The fix is a **one-pole per-sample smoothing** stage inside the DSP, described in the next section.

### Per-Sample DSP Smoothing

Every output-stage gain is smoothed per sample inside the DSP, with a **10 ms one-pole time constant**. Dragging a fader with the mouse therefore produces a continuous amplitude ramp instead of a stepped one, which eliminates the zipper noise that the ear perceives as a staircase in the output level.

#### Location of the smoothed values

- **`ReverbChannel`** (CloudSeed path):
  - `smoothed_dry_out`, `smoothed_predelay_out`, `smoothed_early_out`, `smoothed_line_out`
  - Advanced per sample inside `process()`.
- **`ReverbController`** (shared + FFDN path):
  - `smoothed_width` — used by **both engines** in the final M/S stage.
  - `smoothed_dry_gain` and `smoothed_wet_gain` — used by the **FFDN path only** (the CloudSeed path smooths its own per-channel gains inside `ReverbChannel`).
  - Advanced per sample in `process_cloudseed_inplace` / `process_ffdn_inplace`.
  - Scratch buffers `smooth_width_scratch` and `smooth_dry_scratch` carry the per-sample values into the output helpers.

#### The NaN-snap mechanism

- Smoothed fields are initialized to `f64::NAN`.
- At the top of `process()` (and at the top of the output stage in `ReverbController`), each smoothed field is checked: **if it is `NaN`, it snaps directly to its target** — no ramp.
- `clear_buffers()` resets the smoothed fields back to `NaN`. Preset loads and sleep entry both call `clear_buffers()`, so those transitions take effect instantly. Undo/redo does **not** call `clear_buffers()` (see the undo section), so undo/redo gets a ~10 ms ramp — which is the correct behavior for a parameter edit.

#### Output helpers

- `apply_output_with_width` takes a `widths: &[f64]` slice of per-sample values. It replaced the old `apply_width_and_write`.
- `apply_output_with_dry_and_width` takes both `dry_gains: &[f64]` and `widths: &[f64]`. It replaced the old `apply_width_and_write_with_dry`.
- Both are **static** functions (not methods), because the caller has destructured `self` into disjoint field borrows.

#### Rules for new gains

Any new gain that multiplies the signal inside `process()` (or inside the FFDN / CloudSeed sub-paths) **must** be added to the corresponding smoothing stage:

1. Add a `target_field` to the struct (the value set by `set_parameter` / `set_texture_parameter` / the setters).
2. Add a `smoothed_field` initialized to `f64::NAN` in `new()`.
3. Reset `smoothed_field` to `f64::NAN` inside `clear_buffers()`.
4. Advance `smoothed_field` per sample inside `process()` (or the output loop) with `+= smooth_coeff * (target - smoothed)`.
5. Check for `NaN` at the top of the loop and snap to target if present.
6. Use `smoothed_field` — never `target_field` — in the per-sample multiplication.

If the gain is a raw `mul` on the output without a `smoothed_*` counterpart, the fader will zipper on manual drag.

### `apply_undo_snapshot`: Do **Not** Reset the DSP

- `apply_undo_snapshot` in `editor.rs` must **never** set `reset_requested`. Undo/redo is a context switch between parameter states, not a preset load. Clearing the reverb tail on every undo is jarring and incorrect.
- The comment in the function explains this. If you find a reason to change it, open a discussion first.
- The function calls `apply_texture_params` with the snapshot's complete arrays wrapped in `Some(..)`. This intentionally reuses the same partial-application primitive that the XML loader uses, so there is exactly **one** texture-apply path in the codebase. Do not introduce a second full-array apply just for undo — that duplication is what caused texture state to drift between the two paths in an earlier version.

### FFDN Delay Buffer Sizing

- `SingleNetwork::new` computes `max_delay_samples = ceil(sample_rate × 0.5)`, i.e. a **500 ms** buffer at any sample rate. It used to be a hard-coded `24000`, which capped the usable Size range at 250 ms at 96 kHz.
- `recompute_delays` **must** clamp the computed delay to `self.lines[i].max_delay_samples()` **before** storing it in `delays_samples`. `recompute_feedback` reads its delay time from `delays_samples`; if that value exceeds the buffer capacity, the audible decay becomes shorter than requested.

### Velvet-Noise Filter: Rate-Aware Construction

- `VelvetNoiseFilter` must be created with `with_seed_and_rate(seed, sample_rate)`, **not** `with_seed`. The rate-aware constructor scales the pulse count, the LP coefficient (a true 3.5 kHz at any rate), and the maximum RRS width to the current sample rate.
- `ReverbChannel::set_samplerate` must call `velvet_in.set_sample_rate(sr)` and `velvet_out.set_sample_rate(sr)` so that host sample-rate changes rebuild the internal sequences.
- `set_tone` must store `self.tone` so that `set_sample_rate` can recompute `rrs_width` without losing the user's setting.

### FFDN Character: `velvet_saturate` Has No Makeup Gain

- `velvet_saturate` uses `pre_gain = 1 + amount * 0.8` and **no makeup term**. The previous version had a `1 + 0.15·amount²` small-signal gain, which made any delay line with `g_mid > 0.87` unstable (e.g. `decay = 3 s`, `size ≈ 50 ms` gives `g_mid ≈ 0.89`). Removing the makeup restores unity small-signal gain; mid-level compression is exactly what a saturation stage should do.
- If you reintroduce a makeup term, you **must** also clamp the effective loop gain and add a test like `drive_one_keeps_network_stable`.

### FFDN Dynamics: Audibility

These stages were specifically tuned so the user can actually hear them (previously they were nearly inaudible at any setting):

- **Attack:** time uses `a.sqrt()` (not linear `a`) so the effect grows quickly with the amount knob. Attenuation is a **blend** between "no attenuation" (`1.0`) and "ratio attenuation" (`gain_follower / peak_follower`), driven by `amount`. This is what makes the stage audible at `amount = 0.5` and above.
- **Ducking:** uses a dB-mapped exponential (`-24 × amount × env_norm`), **not** the linear `1 - amount × env` formula. The linear formula gave at most about -3 dB at `amount = 1`, which was imperceptible.
- **Gate:** the threshold range is `[1e-3, 3.16e-1]` (-60 dBFS to -10 dBFS), **not** the old `[1e-4, 3e-2]` (-80 dBFS to -30 dBFS). The old maximum was below typical FFDN wet levels, so a gate at full amount never closed. The fade is **squared** (`(env / thr)²`), producing a decisive close instead of a slow fade.
- All three stages remain **post-network** gain multipliers in `[0, 1]`. None of them can destabilize the FFDN.

### Canonical Factory Presets

- `apply_factory_preset` **must** call `reset_texture_extras`, `reset_velvet`, `reset_ffdn_card_masters`, and `apply_eq_master(true)` **before** applying the preset's own parameters.
- This ensures every factory preset produces a complete, canonical state, independent of what the user was doing before. Without it, loading "Vocal Presence" would keep a Velvet diffusion value from the previous preset, which is surprising and produces inconsistent sounds.
- Workflow state (bypass, freeze, interp, theme, show_popups, window size) is intentionally **not** reset. These are preferences, not preset state.

### Partial XML Presets

- `ParsedPreset` has four independently-optional parameter blocks:
  - `cloudseed: [Option<f32>; 49]`
  - `texture: [Option<f32>; 32]`
  - `enabled: [Option<bool>; 6]`
  - `ffdn: [Option<f32>; 37]`

  plus `illusion_seen: bool`, which gates the 8-value ILLUSION block.
- `cs_seen` was removed: `any_param_seen` is now derived from the `Option` arrays themselves. A completely empty `<CloudReverb></CloudReverb>` is still rejected.
- Loading a preset from a file must use the **partial** apply functions:
  - `apply_cloudseed_partial(p, s, &cloudseed)`
  - `apply_texture_params(p, s, &texture, &enabled)`
  - `apply_ffdn_params(p, s, &ffdn)`
  - `apply_illusion_params(p, s, ..., illusion_seen)`

  `apply_preset` is still used for **factory** presets, which are always complete.
- **Do not** call `apply_texture_params` with an all-zero `[f32; 32]` when a `<TextureRack>` block was absent. This was the source of a historical bug: a file with only `<FFDN>` would silently reset every texture parameter and disable every layer. The partial signature (`[Option<f32>; 32]`, `[Option<bool>; 6]`) makes this mistake impossible at the type level.
- The **expected** behavior for a partial file is: unspecified slots are left untouched, specified slots are overwritten. This applies per parameter, not per section: a `<TextureRack>` block that only contains 5 of the 32 parameters must not reset the other 27.

### ILLUSION (formerly CHIME)

- The file is `texture/illusion.rs`, the struct is `Illusion`, the params struct is `IllusionParams`, the XML tag for enable is `IllusionEnabled`, and the params array constant in `editor.rs` is `ILLUSION_PARAM_NAMES`. There is **no** `Chime*` reference in the current code.
- `knob_row_compact` in `draw_illusion_card` uses `count = 8`, matching the 8 knobs. A mismatch shifts the row sideways.

### Texture Rack: Anti-Click

- Every layer uses `clear_buffers()` on a rising edge (`mix: 0 → >0`), **not** `reset_runtime_state()`. `reset_runtime_state` would leave stale audio in the internal buffers, which produces an audible click when the layer re-enables after a long bypass (e.g. TX kill).

### `MatrixKind` Stays Inline

- `MatrixKind` is an inline enum (`Householder(HouseholderMatrix)` / `Velvet(VelvetMatrix)`), **not** `Box<dyn FeedbackMatrix>` and **not** `Box<VelvetMatrix>`. Clippy's `large_enum_variant` lint fires on this enum because `VelvetMatrix` is ~400 bytes; the lint is explicitly allowed with `#[allow(clippy::large_enum_variant)]`.
- The rationale: `rebuild_matrix` runs on the audio thread whenever the user moves the `FfdnMatrix` or `FfdnLines` knob. The inline enum makes that assignment a stack memcpy. Boxing the variant would introduce a heap allocation there and break RT-safety.
- Do **not** "fix" this lint by boxing. If a future refactor genuinely needs heap indirection (e.g. adding a third matrix type that is much larger), the allocation must be moved out of `rebuild_matrix` first.

### DSP Loops: Iterators Over Index Loops

- Clippy enforces `needless_range_loop` under `-D warnings`. In DSP hot paths, prefer iterators (`iter_mut().zip()`, `iter().take(n).enumerate()`) over manual `for i in 0..n { arr[i] ... }` loops where the code is still clear.
- Iterator chains such as `zip` typically let the compiler elide bounds checks in release builds, so this is a readability change with bit-identical output.
- In test code (`#[cfg(test)]`), `#[allow(clippy::needless_range_loop)]` on the individual test function is preferred over refactoring the test into iterator style — matrix-construction tests are clearer with `a[row][col]` indexing.

---

## Code Style

We enforce strict code quality to keep the project maintainable:

- **Format:** Run `cargo fmt --all` before committing.
- **Lints:** Run `cargo clippy --workspace --all-targets -- -D warnings` and fix all warnings. The `--workspace` and `--all-targets` flags are mandatory: the first checks both crates, the second also checks test code. Omitting either can hide a regression.
- **Lint exceptions:** Lints that would conflict with a hard project rule (e.g. `large_enum_variant` on `MatrixKind`, `too_many_arguments` on the eight-argument knob/fader/numbox helpers) are allowed at the specific site with a comment explaining the reason. Do not disable a lint crate-wide.
- **Comments:** Keep comments concise (1–3 lines). Avoid excessive or dead comments. Never mention plugin version numbers inside code comments.
- **Naming:** Follow standard Rust naming conventions.
- **Param IDs vs. XML names:** For FFDN, `Saturation` / `SaturationEnabled` (param IDs in Rust) deliberately differ from `FfdnDrive` / `FfdnDriveEnabled` (XML names). **Do not** change either side; both are part of the automation and preset ABI.
- **Plugin IDs:** `VST3_CLASS_ID` and `CLAP_ID` are **immutable**. Changing them breaks every saved project.

---

## Pull Request Process

1. **Fork** the repository and create a branch from `main`:

   ```bash
   git checkout -b feature/my-new-feature
   ```

2. Write your code, following the real-time and DSP-specific rules above.
3. **Add tests.** If you modify DSP logic, add unit tests in the `#[cfg(test)]` module. Any change to `density`, `decorr`, `attack`, `ducking`, `gate`, `loop_hp`, `loop_lp`, `mid_focus`, `drive`, or `velvet_saturate` must ship with a `*_is_bit_identical` or `*_keeps_network_stable` test.
4. Run `cargo fmt --all` and `cargo clippy --workspace --all-targets -- -D warnings`.
5. Run the full test suite:

   ```bash
   cargo test -p fdn_reverb_core
   ```

6. Commit your changes with a descriptive message, following a lightweight conventional style:

   | Prefix      | Use for                                   |
   | ----------- | ----------------------------------------- |
   | `feat:`     | New features                              |
   | `fix:`      | Bug fixes                                 |
   | `refactor:` | Internal changes with no behavior change  |
   | `docs:`     | Documentation                             |
   | `test:`     | Test additions                            |

7. Push to your fork and open a **pull request** against the `main` branch.
8. Wait for a maintainer to review your code. We may ask for changes before merging.

---

## Testing Checklist for PRs

Before opening a PR, please verify:

- [ ] `cargo test -p fdn_reverb_core` passes (all `*_is_bit_identical` tests included).
- [ ] `cargo build -p Pejvak_RVB --release` succeeds.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` produces no warnings.
- [ ] If you touched any DSP code: an `*_is_bit_identical` test proves that the neutral/default value is unchanged.
- [ ] If you changed any parameter's default value: XML presets still round-trip correctly (save → load → compare).
- [ ] If you touched the FFDN: `drive_one_keeps_network_stable` and `dynamics_keep_network_stable` still pass.
- [ ] If you touched the `apply_factory_preset` path: every existing factory preset still sounds identical (or the change was approved by a maintainer).
- [ ] If you touched `ParsedPreset`, `xml_to_params`, or any `apply_*_params` function: all five XML shapes below load correctly.
  - Full XML (all sections present)
  - CloudSeed-only XML (no `<FFDN>`, no `<Illusion>`, no `<TextureRack>`)
  - FFDN-only XML (no CloudSeed params, no `<TextureRack>`)
  - Illusion-only XML (no other sections)
  - Incomplete `<TextureRack>` XML (only a subset of the 32 texture parameters)

  In every partial case, the **unspecified** parameters must be **preserved** (not reset to 0/false).
- [ ] If you added or changed an output-stage gain: it has a `smoothed_*` field, is reset to `NaN` in `clear_buffers()`, and is advanced per sample inside `process()`.
- [ ] **Manual fader drag test:** dragging the affected fader produces no zipper noise (use a sustained reverb tail).
- [ ] **Preset load test:** after loading a preset, the smoothed gains snap instantly (no 10 ms glide).
- [ ] **Undo/redo test:** pressing Undo immediately after a manual fader drag restores the previous value with a ~10 ms ramp, **without** cutting the reverb tail.

---

## Known Issues

These are documented so contributors don't spend time "fixing" the same issue twice.

### Logo quality in the About modal

`manual/logo.png` is currently low-resolution (~150×200 px) and is upscaled to `64.0 × scale` logical pixels in the About modal, producing a pixelated look. This is a **file** issue, not a code issue: replacing the file with a 1024×1024 transparent PNG fixes it with zero code changes. Do not add image-processing code to work around this.

### `SAMPLE_ACCURATE_AUTOMATION`

See [Zipper Noise: Two Separate Fixes](#zipper-noise-two-separate-fixes). If a PR enables it, the PR will be rejected unless it also adds `.with_smoother()` to every DSP-facing `FloatParam` and updates every `process_inner` path to consume the smoothed value.

---

## License

By contributing to PEJVAK RVB, you agree that your contributions will be licensed under the **GNU General Public License v3.0 or later (GPL-3.0-or-later)**.

Thank you for contributing to the open-source audio community!
