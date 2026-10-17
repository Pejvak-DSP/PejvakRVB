\# PEJVAK RVB



> Hybrid Algorithmic Reverb — CloudSeed + FFDN in one plugin.



PEJVAK RVB is a professional algorithmic reverb plugin that combines two

independent DSP engines in a single package. Written in pure Rust with

zero allocation on the audio thread.



\## Features



\- \*\*CloudSeed Engine\*\* — faithful `f64` port of Valdemar Erlingsson's

&#x20; CloudReverb (legacy-v1), bit-identical to the original C++ DSP.

\- \*\*FFDN Engine\*\* — modern Filtered Feedback Delay Network with

&#x20; Householder / Velvet matrices, frequency-dependent attenuation, and

&#x20; character controls (saturation, tilt, cross-feed).

\- \*\*Texture Rack\*\* — six post-reverb layers available on both engines.

\- \*\*ILLUSION\*\* — granular cloud with pitched overtones.

\- \*\*Undo / Redo\*\* — 50 steps with parameter-name tooltips.

\- \*\*96 factory presets\*\* across 16 categories.



\## Installation



\### Windows

\- CLAP: copy to `C:\\Program Files\\Common Files\\CLAP\\`

\- VST3: copy to `C:\\Program Files\\Common Files\\VST3\\PEJVAK RVB\\`



\### macOS (universal binary)

\- VST3: copy to `\~/Library/Audio/Plug-Ins/VST3/`

\- CLAP: copy to `\~/Library/Audio/Plug-Ins/CLAP/`



\### Linux

\- VST3: copy to `\~/.vst3/`

\- CLAP: copy to `\~/.clap/`



\## Building from source



```bash

cargo build -p Pejvak\_RVB --release

cargo nice-plug bundle Pejvak\_RVB --release

```



\## License



GPL-3.0-or-later. See \[LICENSE](LICENSE) for the full text.



\## Credits



\- \[CloudSeed](https://github.com/ValdemarOrn/CloudSeed/tree/legacy-v1) by Valdemar Erlingsson

\- \[CloudReverb](https://github.com/xunil-cloud/CloudReverb) by xunil-cloud



Made with ♥ by Amin Yousefinejad.

