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
- [Platform & UI Rules (Window Resizing & Keyboard)](#platform--ui-rules-window-resizing--keyboard)
- [Security Rules (Non-Negotiable)](#security-rules-non-negotiable)
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

### Running the Tests

```bash
# 42 DSP tests
cargo test -p fdn_reverb_core

# 12 plugin tests (XML parser, parameter handling, updater, string parsing)
cargo test -p Pejvak_RVB
```

### Linting the Whole Workspace

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

`--all-targets` is essential: without it, test code is not linted, and violations there go unnoticed until CI catches them. `--workspace` ensures both `fdn_reverb_core` and `Pejvak_RVB` are checked.

---

## Strict Code Rules (Audio Thread Safety)

Audio programming is highly sensitive. A single allocation on the audio thread or a bad lock can cause dropouts or crashes in a DAW. All pull requests must adhere to the following rules.

### 1. Real-Time Safety (Zero Allocation)

- Never allocate memory (`Vec::new`, `Box::new`, `String::from`, etc.) inside `process()` or any DSP function called from it.
- If you need dynamic memory, pre-allocate it in `new()` or `activate()`.
- Use fixed-size arrays (e.g. `[f64; 16]`) or pre-allocated buffers instead of `Vec` in the audio hot path.
- All `ctrl.set_*` calls (e.g. `set_parameter`, `set_ffdn_*`, `set_texture_parameter`, `set_illusion_*`) are made from inside `process_inner`, i.e. on the audio thread. They must remain allocation-free. `MatrixKind` is stored inline precisely so that `rebuild_matrix` can reassign it without touching the heap.

### 2. Pure f64 Math

- All DSP operations must be done in `f64` (double precision).
- `f32` may only be used at the very boundary (plugin input/output buffers).
- Never use fast-math or approximate fast-trig functions unless they have been tested and approved for this project.
- No FMA, no reassociation. The `wide` SIMD crate is used only for element-wise mul/add on independent data, so that results stay bit-identical to scalar code.

### 3. No Locking on the Audio Thread

- Never use `Mutex`, `RwLock`, or any thread-blocking operation inside `process()`.
- Use atomics (`AtomicBool`, `AtomicU64`) or lock-free structures for communication between the UI and the audio thread.
- On the UI side, when interacting with egui memory, avoid nested locks (e.g. calling `ui.ctx().input()` inside `ui.memory_mut()`), as this can deadlock. This is explicitly required by the TX kill switch (`pejvak_tx_kill_active` and `pejvak_force_undo_commit_at`).

### 4. Prevent Denormals

- Recursive filters must flush denormals (values below `1e-30` are treated as `0.0`).
- ILLUSION already does this (`flush_denorm`); any new recursive filter must do the same.

### 5. Panic Isolation on the Audio Thread

The entire `process()` body is wrapped in `std::panic::catch_unwind`. Any new code path that could panic must be aware of this latch:

- **First caught panic:** every DSP buffer is wiped, every parameter cache is invalidated, and processing continues.
- **Second caught panic (before a host `activate()` or `reset()`):** the engine latches to permanent silence (`engine_dead = true`).

If a new code path needs to panic on user input (it should not), ensure that the panic message is short and that the two-strike latch is enough. Never assume infinite retries.

---

## DSP-Specific Rules (Non-Negotiable)

These rules are specific to this project and must be preserved by every PR. Each one was learned the hard way, usually after a bug that cost hours of debugging or shipped in a release.

### Zipper Noise: Two Separate Fixes

Zipper noise in this plugin comes from two independent sources, and each has its own fix. Do not confuse them.

**1. Host automation → `SAMPLE_ACCURATE_AUTOMATION = false`**

`SAMPLE_ACCURATE_AUTOMATION` must remain `false` unless every `FloatParam` that feeds the DSP has `.with_smoother()` enabled and every path in `process_inner` reads `smoothed.next()` (per sample) or `smoothed.next_block()` (per sub-block).

With `SAMPLE_ACCURATE_AUTOMATION = true` and un-smoothed parameters, nice-plug splits the host buffer at every automation event. This produces a vertical step in the parameter value at each split point, heard as zipper noise on automated parameters.

Setting it to `false` avoids the split entirely: `process_inner` sees a full host buffer, `value()` is constant within the block, and no step can occur.

Re-enabling it requires the `.with_smoother()` migration described above. Do not flip the flag without that migration.

**2. Manual fader movement → per-sample DSP smoothing**

Manual fader dragging is a separate problem: even with a full host buffer, each UI frame produces a stepwise parameter change that the DSP would apply as a step in amplitude. The fix is a one-pole per-sample smoothing stage inside the DSP, described in the next section.

### Per-Sample DSP Smoothing

Every output-stage gain is smoothed per sample inside the DSP, with a 10 ms one-pole time constant. Dragging a fader with the mouse therefore produces a continuous amplitude ramp instead of a stepped one.

Rules for new gains:

1. Add a `target_field` to the struct.
2. Add a `smoothed_field` initialized to `f64::NAN` in `new()`.
3. Reset `smoothed_field` to `f64::NAN` inside `clear_buffers()`.
4. Advance `smoothed_field` per sample inside the loop.
5. Check for NaN at the top of the loop and snap to target if present.
6. Use `smoothed_field` (never `target_field`) in the per-sample multiplication.

### Numeric Parameter Round-Trip: ULP Walking

Several parameters use `(p * A) as i64` truncation inside `scaled_parameter_value` (PreDelay, TapLength, etc.). For these, plain bisection converges to a normalised value just under the exact crossing point, dropping the fractional part.

`parse_parameter` and `denormalize` must walk f32 ULPs outward from the bisection midpoint until the value round-trips exactly. Add a round-trip test for any new discrete parameter.

### `apply_undo_snapshot`: Do Not Reset the DSP

`apply_undo_snapshot` in `editor/xml.rs` must never set `reset_requested`. Undo/redo is a context switch between parameter states, not a preset load. Clearing the reverb tail on every undo is jarring.

The function calls `apply_texture_params` with the snapshot's complete arrays wrapped in `Some(..)` to reuse the partial-application primitive.

### FFDN, Velvet-Noise & Dynamics Constraints

- **FFDN Delay Buffer:** `max_delay_samples = ceil(sample_rate × 0.5)`. `recompute_delays` must clamp the computed delay to this limit before storing.
- **Delay Buffer Defensive Clamps:** Do not remove the safety clamps in `ModulatedAllpass`, `ModulatedDelay`, and `MultitapDiffuser`. They protect `unsafe` blocks.
- **Velvet-Noise:** Construct with `with_seed_and_rate(seed, sample_rate)`. `set_tone` must store `self.tone` so `set_sample_rate` can recompute `rrs_width`.
- **`velvet_saturate`:** No makeup gain. The small-signal gain must remain unity to prevent infinite feedback growth.
- **Dynamics:** Attack, Ducking, and Gate use specific nonlinear mappings (sqrt, dB-mapped exponential, squared fade) for audibility. Do not revert them to linear mappings.
- **`MatrixKind` Stays Inline:** Do not box `VelvetMatrix` (even if clippy suggests it). `rebuild_matrix` runs on the audio thread and must not allocate.

---

## Platform & UI Rules (Window Resizing & Keyboard)

### Window Resizing (v1.2.0)

- **Global Persistence:** Window size is saved globally in `window_size.cfg` (in the `user_presets_dir()`) and is read during `PejvakRvbParams::default()`.
- **Native Aspect Ratio:** `ResizableWindow` is removed. The UI enforces a 1100:570 aspect ratio via `ui.ctx().send_viewport_cmd(InnerSize)`.
- **`any_released()` Condition:** Only send the resize command when `ui.input(|i| i.pointer.any_released())` is true. Sending it continuously causes menus to close unexpectedly when the mouse hovers over them.
- **`NativeSize`:** Use struct literal `NativeSize { width, height }`. `NativeSize::Logical` does not exist in nice-plug 0.4.2.

### Keyboard Forwarding to DAW (v1.2.0)

- **No `SendInput`:** Do not use `SendInput` or `SetForegroundWindow`. This causes infinite Play/Stop loops in Reaper (the DAW echoes the key back to the plugin).
- **`PostMessageW`:** Use `PostMessageW` directed to the DAW's parent window: `GetParent(GetFocus())`.
- **Loop Prevention:** Use the `LAST_FORWARDED_KEY` static `Mutex`. Ignore any incoming `Event::Key` if it matches the key forwarded within the last 50 milliseconds.
- **Scancodes & Extended Flags:** Numpad keys must be sent with `extended = false`. Navigation keys (Delete, Home, Arrows) must be sent with `extended = true`.
- **`text_edit_active` Fix:** In `read_text_edit_active`, force a return of `false` if `m.focused().is_none()`. This prevents the keyboard forwarding from getting stuck.

### egui 0.37 Compatibility

- Replace `Id::new` with `Id::unique`.
- Replace `screen_rect()` with `viewport_rect()`.
- The `egui::Key::Asterisk` variant does not exist. Handle the `*` key via `egui::Event::Text("*")`.

### TX Kill Switch & Menus

- **TX Kill Switch:** It must be possible to engage the TX lock even if all layers are already disabled (the `any_enabled` guard has been removed).
- **Menu Closures:** Clicking "Show pop-ups" must close the menu. Trigger `clicked_item` in the toggle row.

---

## Security Rules (Non-Negotiable)

### No Shell-Out for Opening Files or URLs

Never call `cmd /c start "" "<path>"` on Windows. Use `ShellExecuteW` from `windows-sys` instead. See `manual.rs::open_path` and `updater.rs::open_url`. Never build a shell command by string concatenation.

### HTTPS-Only Network

The updater uses `ureq::AgentBuilder::new().https_only(true)`. Do not remove this. Any new outbound network request must go through `http_agent()`.

### SHA-256 Verification of Downloads

Every installer downloaded by `download_installer` is hashed while it streams, then compared against GitHub's immutable `digest` field. A mismatch deletes the file and returns an error. Do not add any "skip verification" flags.

### Explicit User Confirmation Before Running an Installer

`launch_installer` must only be called after an explicit user confirmation (checkbox + button). The plugin never auto-runs a downloaded binary.

### Updater Threads Must Not Latch

Wrap updater thread bodies in `std::panic::catch_unwind(AssertUnwindSafe(...))`. Release the `update_busy` flag on both the success and panic paths.

### XML Parser Hardening

The XML parser never allocates based on file contents. Do not use `serde` or any XML crate that resolves external entities (XXE).

### Parameter String Parsing

All text input goes through `parse_parameter` or one of the `*_parser()` functions. Never pass raw user text to a shell or an eval-like construct.

---

## Code Style

- **Format:** Run `cargo fmt --all` before committing.
- **Lints:** Run `cargo clippy --workspace --all-targets -- -D warnings` and fix all warnings.
- **Lint exceptions:** Allowed at the specific site with a comment (e.g. `large_enum_variant` on `MatrixKind`).
- **Comments:** Keep comments concise (1–3 lines). Never mention plugin version numbers inside code comments.
- **Naming:** Follow standard Rust naming conventions.
- **Param IDs:** `Saturation` / `SaturationEnabled` are the Rust param IDs; `FfdnDrive` / `FfdnDriveEnabled` are the XML names. Both are immutable.
- **Plugin IDs:** `VST3_CLASS_ID` and `CLAP_ID` are immutable.

---

## Pull Request Process

1. Fork the repository and create a branch from `main`.
2. Write your code, following the real-time and DSP-specific rules above.
3. Add tests. If you modify DSP logic, add `*_is_bit_identical` or `*_keeps_network_stable` tests.
4. Run `cargo fmt --all` and `cargo clippy --workspace --all-targets -- -D warnings`.
5. Run the full test suite (`cargo test -p fdn_reverb_core` and `cargo test -p Pejvak_RVB`).
6. Commit your changes using conventional prefixes (`feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `sec:`).
7. Push to your fork and open a pull request against `main`.

---

## Testing Checklist for PRs

- [ ] `cargo test -p fdn_reverb_core` passes (all 42 tests).
- [ ] `cargo test -p Pejvak_RVB` passes (all 12 tests).
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` produces no warnings.
- [ ] If you touched any DSP code: an `*_is_bit_identical` test proves that the neutral/default value is unchanged.
- [ ] If you touched the FFDN: `drive_one_keeps_network_stable` and `dynamics_keep_network_stable` still pass.
- [ ] If you touched `ParsedPreset` or `xml_to_params`: all 5 XML partial-load shapes work correctly.
- [ ] If you added a new output-stage gain: it has a `smoothed_*` field and uses the NaN-snap mechanism.
- [ ] If you added a network request: it goes through `http_agent()` with `https_only(true)`.
- [ ] Manual fader drag test: dragging the affected fader produces no zipper noise.
- [ ] Window resize test: resizing the window on all corners scales the UI uniformly without letterboxing.
- [ ] Keyboard forwarding test: pressing Space (Play/Stop) in the DAW does not cause a flickering loop in Reaper. All Numpad operators work in Cakewalk/Sonar.
- [ ] TX kill switch test: it toggles successfully even when all texture layers are already disabled.
- [ ] Menu test: "Show pop-ups" closes the menu immediately upon clicking.

---

## Known Issues

- **Studio One Window Size:** Studio One may still open the window slightly small. This requires a DAW-level workaround.
- **Logo Quality:** `manual/logo.png` is low-resolution. Replacing it with a 1024×1024 transparent PNG fixes the pixelation with zero code changes.
- **Code Signing:** The Windows `.exe` and macOS `.pkg` are not Authenticode- or Developer-ID-signed. SmartScreen and Gatekeeper will warn users on first run.

---

## License

By contributing to PEJVAK RVB, you agree that your contributions will be licensed under the GNU General Public License v3.0 or later (GPL-3.0-or-later).

Thank you for contributing to the open-source audio community!
