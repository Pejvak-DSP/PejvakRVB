***

```markdown
# Contributing to PEJVAK RVB

First off, thank you for considering contributing to **PEJVAK RVB**! It's people like you that make open-source audio software such a powerful and inspiring community.

This document outlines the guidelines for contributing to this project, whether it's reporting a bug, suggesting a new feature, or submitting a pull request.

---

## 📜 Code of Conduct

By participating in this project, you are expected to uphold a welcoming, respectful, and harassment-free environment. We are all here to build great audio tools.

---

## 🐛 Reporting Bugs

If you find a bug, please open an issue on our [GitHub Issues](https://github.com/Pejvak-DSP/PejvakRVB/issues) page. 

To help us fix it quickly, please include:
1. **Your Environment:** OS version (Windows/macOS/Linux), CPU architecture (x86_64 / Apple Silicon), and DAW name/version.
2. **Plugin Version:** The exact version of PEJVAK RVB you are using (visible in the `☰ → About` menu).
3. **Steps to Reproduce:** A clear, step-by-step description of how to make the bug happen.
4. **Expected vs. Actual Behavior:** What you thought would happen versus what actually happened.

---

## 💡 Suggesting Features

We welcome feature requests! Before opening an issue, please check if a similar request already exists. 
When suggesting a feature, explain *why* it would be useful, especially from a DSP, musical, or UI/UX perspective.

---

## 🛠️ Development Setup

### Prerequisites
- **Rust 1.98+** (Install via [rustup](https://rustup.rs))
- **Windows:** Visual Studio Build Tools (C++).
- **macOS:** Xcode Command Line Tools (`xcode-select --install`).
- **Linux:** X11 and OpenGL development libraries:
  ```bash
  sudo apt-get install -y libx11-dev libxcursor-dev libxcb-dri2-0-dev \
    libxcb-icccm4-dev libx11-xcb-dev mesa-common-dev \
    libgl1-mesa-dev libglu1-mesa-dev
  ```

### Building
To build the plugin locally:
```bash
cargo nice-plug bundle Pejvak_RVB --release
```

To run the DSP tests:
```bash
cargo test -p fdn_reverb_core
```

---

## ⚠️ Strict Code Rules (Audio Thread Safety)

Audio programming is highly sensitive. A single allocation on the audio thread or a bad lock can cause dropouts or crashes in a DAW. **All Pull Requests must adhere to the following rules:**

### 1. Real-Time Safety (Zero Allocation)
- **NEVER** allocate memory (`Vec::new`, `Box::new`, `String::from`, etc.) inside the `process()` function or any DSP function called within it.
- If you need dynamic memory, pre-allocate it in `new()` or `activate()`.
- Use fixed-size arrays (`[f64; 16]`) or pre-allocated buffers instead of `Vec` inside the audio hot path.

### 2. Pure f64 Math
- All DSP operations must be done in **`f64`** (double precision). 
- `f32` should only be used at the very boundary (plugin input/output buffers).
- **Never** use fast-math or approximate fast-trig functions unless they are tested and approved for this project.

### 3. No Locking in the Audio Thread
- Never use `Mutex`, `RwLock`, or thread-blocking operations inside `process()`.
- Use atomics (`AtomicBool`, `AtomicU64`) or lock-free structures for communication between the UI and the audio thread.
- When interacting with `egui` memory, avoid nested locks (e.g., calling `ui.ctx().input()` inside `ui.memory_mut()`), as it can cause deadlocks.

### 4. Prevent Denormals
- Ensure that recursive filters flush denormals (values below `1e-30` should be treated as `0.0`).

---

## 🧹 Code Style

We enforce strict code quality to keep the project maintainable:
- **Format:** Run `cargo fmt` before committing.
- **Lints:** Run `cargo clippy -- -D warnings` and fix all warnings.
- **Comments:** Write concise comments (1–3 lines). Avoid excessive or dead comments.
- **Naming:** Follow standard Rust naming conventions.

---

## 🚀 Pull Request Process

1. **Fork** the repository and create your branch from `main`:
   ```bash
   git checkout -b feature/my-new-feature
   ```
2. Write your code, ensuring it follows the strict Real-Time rules above.
3. **Add Tests:** If you modify DSP logic, add unit tests in the `#[cfg(test)]` module.
4. Run `cargo fmt` and `cargo clippy`.
5. Commit your changes with a descriptive message (e.g., `feat: add soft-clip to dynamics`, `fix: resolve TX switch crackle`).
6. Push to your fork and submit a **Pull Request** to the `main` branch.
7. Wait for a maintainer to review your code. We might ask for some changes before merging.

---

## 📄 License

By contributing to PEJVAK RVB, you agree that your contributions will be licensed under the **GNU General Public License v3.0 or later (GPL-3.0-or-later)**.

Thank you for contributing to the open-source audio community! 🎛️🎶
```