\# Changelog



All notable changes to PEJVAK RVB are documented in this file.



The format is based on \[Keep a Changelog](https://keepachangelog.com/en/1.1.0/),

and this project adheres to \[Semantic Versioning](https://semver.org/spec/v2.0.0.html).



\## \[1.0.1] - 2026-10-20



\### Fixed



\- \*\*Zipper noise on automation\*\* — `SAMPLE\_ACCURATE\_AUTOMATION` is now

&#x20; disabled. With the previous `true` setting, `nice-plug` split the host

&#x20; buffer at every automation event, producing a vertical step in the

&#x20; parameter value at each split point. This was the actual source of the

&#x20; zipper noise heard on Dry, PreDly, Early, Late, and Width automation.

&#x20; Setting it to `false` processes full host buffers, so `value()` is

&#x20; constant within a buffer and no step can occur. Re-enabling it requires

&#x20; `.with\_smoother()` on every DSP-facing `FloatParam`.



\- \*\*FFDN delay buffer sizing\*\* — `SingleNetwork::new` now uses

&#x20; `max\_delay\_samples = ceil(sample\_rate × 0.5)` (500 ms at any rate)

&#x20; instead of the hard-coded `24000`. At 96 kHz, the old buffer capped

&#x20; the usable Size range at 250 ms.



\- \*\*FFDN feedback computed from clamped delay\*\* — `recompute\_delays`

&#x20; now clamps the computed delay to `max\_delay\_samples()` \*before\*

&#x20; storing it in `delays\_samples`. Previously the raw value was stored,

&#x20; so the feedback gain was calculated for a delay longer than the line

&#x20; could hold — the audible decay came out shorter than requested.



\- \*\*Velvet-Noise filter is now sample-rate aware\*\* — `VelvetNoiseFilter`

&#x20; is constructed with `with\_seed\_and\_rate(seed, sample\_rate)`, which

&#x20; scales the pulse count, the LP coefficient (true 3.5 kHz at any rate),

&#x20; and the maximum RRS width. `ReverbChannel::set\_samplerate` calls

&#x20; `set\_sample\_rate()` on both VN filters so host rate changes rebuild

&#x20; the internal sequences.



\- \*\*FFDN drive stability\*\* — `velvet\_saturate` no longer applies a makeup

&#x20; term. The previous `1 + 0.15·amount²` makeup gave the shaper a

&#x20; small-signal gain of 1.15 at `amount = 1`. Inside the feedback loop,

&#x20; any line with `g\_mid · 1.15 > 1` would grow instead of decay (e.g.

&#x20; `decay = 3 s` and `size ≈ 50 ms` gives `g\_mid ≈ 0.89`). Removing the

&#x20; makeup restores unity small-signal gain; mid-level compression is

&#x20; exactly what a saturation stage should do.



\- \*\*FFDN Dynamics audibility\*\* — the three stages were nearly inaudible

&#x20; at any setting. They are now tuned so the user can hear them:

&#x20; - \*\*Attack\*\* uses a square-root time curve and a blend-based

&#x20;   attenuation (`1 - amount × (1 - ratio)`) so `amount = 0.5` produces

&#x20;   a clearly audible swell.

&#x20; - \*\*Ducking\*\* uses a dB-mapped exponential (`-24 × amount × env\_norm`)

&#x20;   instead of the linear `1 - amount × env` formula (which gave at most

&#x20;   \~-3 dB and was imperceptible).

&#x20; - \*\*Gate\*\* threshold range is widened to `\[-60, -10] dBFS` (was

&#x20;   `\[-80, -30] dBFS`) and the fade is squared. The old maximum was

&#x20;   below typical FFDN wet levels, so a gate at full amount never

&#x20;   actually closed.



\- \*\*Undo/redo no longer cuts the reverb tail\*\* — `apply\_undo\_snapshot`

&#x20; intentionally does not set `reset\_requested`. Undo/redo is a context

&#x20; switch between parameter states, not a preset load — clearing the

&#x20; reverb tail on every undo was jarring.



\- \*\*Preset library canonicalization\*\* — `apply\_factory\_preset` now

&#x20; resets the texture non-mix parameters, Velvet, the seven FFDN card

&#x20; master switches, and the EQ master switch \*before\* applying the

&#x20; preset's own parameters. Every factory preset now produces a complete,

&#x20; canonical state independent of what the user was doing before.



\- \*\*Partial XML preset support\*\* — `ParsedPreset.cloudseed` is now

&#x20; `\[Option<f32>; 49]`. `xml\_to\_params` no longer requires all 49

&#x20; CloudSeed parameters to be present; a file with only FFDN, only

&#x20; Illusion, or a subset of CloudSeed loads non-destructively. A

&#x20; completely empty `<CloudReverb></CloudReverb>` is still rejected by

&#x20; the `any\_param\_seen` check. Loading from a file uses

&#x20; `apply\_cloudseed\_partial`, not `apply\_preset`.



\### Changed



\- \*\*User preset directory scan is throttled\*\* — `user\_preset\_names()`

&#x20; and `user\_presets\_dir()` now run at most every 0.5 seconds instead of

&#x20; every frame. On slow disks with many presets this removes a visible

&#x20; UI stutter.



\- \*\*`current\_preset` is persisted\*\* — the preset name is now stored via

&#x20; `#\[persist = "current\_preset"]`, so closing and reopening the editor

&#x20; (or reloading a host project) shows the correct preset label instead

&#x20; of always reverting to `Init`.



\- \*\*About modal text\*\* — now says "100 factory presets" and "wide 1.6"

&#x20; (both were stale).



\- \*\*Header order\*\* — the actual left-to-right order is:

&#x20; `MAIN|TEXTURE → TX → CLOUD|FFDN → Bypass/Freeze/Interp →

&#x20; Undo/Redo → Preset → ☰ → ☀/☾`.



\### Notes



\- The logo in the About modal is currently pixelated because

&#x20; `manual/logo.png` is low-resolution (\~150×200 px). This is a \*\*file\*\*

&#x20; issue, not a code issue. Replacing the file with a 1024×1024

&#x20; transparent PNG fixes it with zero code changes.



\## \[1.0.0] - 2025-XX-XX



Initial public release.



\- CloudSeed legacy-v1 port (49 parameters, pure `f64`).

\- FFDN engine with Householder and Velvet matrices.

\- ILLUSION granular cloud with pitched overtones.

\- Six-layer Texture Rack with reorderable signal chain.

\- Velvet-Noise VFDN diffusion inside CloudSeed.

\- 100 factory presets across 16 categories.

\- Undo/Redo with engine and page restore.

\- TX kill switch, Smart Idle, Smart Sleep.

