<div align="center">

<!-- ===== LOGO ===== -->
<img src="manual/logo-bw.png" alt="Pejvak DSP Logo" width="180" style="margin-bottom: 20px;"/>

# 🎛️ PEJVAK RVB

**A Professional Hybrid Algorithmic Reverb — Dual Engine. Pure Rust.**

[![Version](https://img.shields.io/badge/version-1.0.0-blue?style=for-the-badge&logo=github)](https://github.com/Pejvak-DSP/PejvakRVB/releases)
[![License](https://img.shields.io/badge/license-GPL--3.0--or--later-success?style=for-the-badge)](LICENSE)
[![Rust](https://img.shields.io/badge/Built_with-Rust_🦀-orange?style=for-the-badge&logo=rust)](https://www.rust-lang.org)
[![Platforms](https://img.shields.io/badge/Platforms-Windows_%7C_macOS_%7C_Linux-lightgrey?style=for-the-badge&logo=apple)]()

[![VST3](https://img.shields.io/badge/Format-VST3-8A2BE2?style=flat-square)]()
[![CLAP](https://img.shields.io/badge/Format-CLAP-FF4500?style=flat-square)]()
[![Org](https://img.shields.io/badge/Publisher-Pejvak_DSP-cyan?style=flat-square)](https://github.com/Pejvak-DSP)

</div>

<br>

> **PEJVAK RVB** is a high-end algorithmic reverb plugin written entirely in Rust. It combines two completely independent DSP engines in a single package — a bit-exact port of the legendary CloudSeed, and a modern Filtered Feedback Delay Network (FFDN) — and layers a rich, click-free texture system on top.
>
> **Design Goals:** Pure `f64` DSP, zero allocation on the audio thread, and a UI that feels as good as it sounds.

---

## ✨ Features

### 🌌 Dual Engine Architecture

Switch seamlessly between two distinct worlds of reverb:

| Engine | Character | Highlights |
| :--- | :--- | :--- |
| **☁️ CloudSeed** | Warm, dense, and deep | Bit-exact port of Valdemar Erlingsson's *CloudSeed* (legacy-v1). 49 parameters, deep early-diffusion front-end, and Velvet-Noise diffusion. |
| **🌐 FFDN** | Clean, modern, and characterful | Filtered Feedback Delay Network. Features Householder & Velvet matrices, frequency-dependent attenuation, Velvet saturation, spectral tilt, and 3 dynamics stages (Attack / Ducking / Gate). |

*Switching engines instantly clears the departing engine's buffers, ensuring zero bleed-over from stale tails.*

### 🎨 Texture Rack (6 Layers)

Available on **both** engines. Drag the chips in the Signal Chain to reorder them:

- **Shimmer:** Granular pitch shifter inside a feedback loop.
- **Mist:** Short modulated delay with light feedback.
- **Drift:** Very slow pitch wobble.
- **Crystal:** Dual grain readers in a shared buffer.
- **Scatter:** Overlapping grains with random start positions.
- **Bloom:** Slow-rising reverse-reverb character.

*Every layer features a 20 ms anti-click smoother and resets its buffers on the rising edge from zero.*

### 🔮 ILLUSION — Granular Cloud

A beautiful granular cloud with pitched overtones. Features 8 parameters (Spark, Still, Aura, Veil, Wide, Tint, Rate, Grain) and 8 independent anti-click techniques, including Catmull-Rom cubic interpolation and soft-clip saturation.

### 🚀 Everything Else

- 🕹️ **Drag-and-Drop Signal Chain** with smooth animations and cursor feedback.
- ↩️ **Undo / Redo:** 50 steps with parameter-name tooltips.
- 🛑 **TX Kill Switch:** One-click master switch for all texture layers.
- 🎛️ **96 Factory Presets** across 16 color-coded categories.
- 🎨 **Dark / Light theme**, resizable UI (70% – 200%).
- 📖 **Bundled HTML Manual:** Features a sticky header, live search, and beautiful layout.

---

## 📸 Screenshots

<div align="center">
  <img src="screenshots/main.png" alt="CloudSeed Engine" width="800" style="border-radius: 8px; box-shadow: 0 4px 8px rgba(0,0,0,0.2); margin-bottom: 20px;"/>
  <br>
  <i>CloudSeed Engine — Warm and Dense</i>
</div>

<br>

<div align="center">
  <img src="screenshots/ffdn.png" alt="FFDN Engine" width="800" style="border-radius: 8px; box-shadow: 0 4px 8px rgba(0,0,0,0.2); margin-bottom: 20px;"/>
  <br>
  <i>FFDN Engine — Modern and Characterful</i>
</div>

<br>

<div align="center">
  <img src="screenshots/texture.png" alt="Texture Rack" width="800" style="border-radius: 8px; box-shadow: 0 4px 8px rgba(0,0,0,0.2);"/>
  <br>
  <i>Texture Rack & ILLUSION</i>
</div>

---

## 📦 Installation

### 🪟 Windows

Copy the plugin to the standard system folders:

```powershell
# CLAP
Copy-Item Pejvak_RVB.clap "C:\Program Files\Common Files\CLAP\"

# VST3
New-Item -ItemType Directory -Force -Path "C:\Program Files\Common Files\VST3\PEJVAK RVB"
Copy-Item Pejvak_RVB.vst3 "C:\Program Files\Common Files\VST3\PEJVAK RVB\"
```

### 🍎 macOS (Universal Binary)

Runs natively on both Intel and Apple Silicon.

```bash
cp -R "PEJVAK RVB.vst3" ~/Library/Audio/Plug-Ins/VST3/
cp -R "Pejvak_RVB.clap" ~/Library/Audio/Plug-Ins/CLAP/
```

> **Note:** If macOS blocks the plugin, go to **System Settings → Privacy & Security → Open Anyway**.

### 🐧 Linux

```bash
# VST3
mkdir -p ~/.vst3/"PEJVAK RVB.vst3"/Contents/x86_64-linux
cp "PEJVAK RVB.vst3/Contents/x86_64-linux/PEJVAK RVB.so" ~/.vst3/"PEJVAK RVB.vst3"/Contents/x86_64-linux/

# CLAP
mkdir -p ~/.clap
cp Pejvak_RVB.clap ~/.clap/
```

---

## 🛠️ Building from Source

Ensure you have **Rust 1.98+** installed.
(Linux users also need X11 and OpenGL development libraries.)

```bash
git clone https://github.com/Pejvak-DSP/PejvakRVB.git
cd PejvakRVB
cargo nice-plug bundle Pejvak_RVB --release
```

The bundled plugins will appear in `target/bundled/`.

### 🍏 macOS Universal Binary

```bash
rustup target add aarch64-apple-darwin x86_64-apple-darwin
cargo build --target aarch64-apple-darwin --release
cargo build --target x86_64-apple-darwin --release

mkdir -p target/universal/release
lipo -create \
  target/aarch64-apple-darwin/release/libPejvak_RVB.dylib \
  target/x86_64-apple-darwin/release/libPejvak_RVB.dylib \
  -output target/universal/release/libPejvak_RVB.dylib
```

---

## 🧱 Project Structure

```text
PejvakRVB/
├── fdn_reverb_core/        ← Pure DSP core (CloudSeed, FFDN, Textures)
│   └── src/
│       ├── cloudseed/      ← Bit-exact CloudSeed port
│       ├── ffdn/           ← Filtered Feedback Delay Network
│       └── texture/        ← Six texture layers + ILLUSION
├── Pejvak_RVB/             ← Plugin wrapper (params, UI, presets)
│   └── src/
│       ├── lib.rs          ← Plugin definition & Audio thread
│       ├── editor.rs       ← egui interface
│       ├── manual.rs       ← HTML manual loader
│       └── presets.rs      ← 96 factory presets
├── manual/                 ← Bundled HTML manual + logo
└── Cargo.toml              ← Workspace root
```

---

## 🙏 Credits

PEJVAK RVB stands on the shoulders of giants. Huge thanks to:

- **CloudSeed** — Valdemar Erlingsson (the original C++ reference)
- **CloudReverb** — xunil-cloud (JUCE fork & modern parameter layout)
- **FFDN Theory** — Fagerström et al. (DAFx 2020) and Schlecht & Habets (IEEE TSP 2017)
- **Granular Design** — Curtis Roads, *Microsound* (MIT Press, 2001)

Built with ❤️ using Rust, `nice-plug`, `egui`, and `wide`.

---

## 📄 License

This project is licensed under the **GNU General Public License v3.0 or later**.

```text
Copyright (C) 2025 Pejvak DSP
Author: Amin Yousefinejad
```

See the [LICENSE](LICENSE) file for the full text.

<div align="center">

**Pejvak DSP** · Designed & developed by **Amin Yousefinejad**

<i>Made with ♥ for the audio community.</i>

</div>
