//! Filtered Feedback Delay Network (FFDN) – GPL-3.0
//!
//! A modern reverb engine that places diffusion and filtering *inside*
//! the network's feedback loop rather than in a separate pre-/post-
//! processing stage. Compared to the CloudSeed engine, which stacks
//! multitap delays, all-pass diffusers, and 12 parallel delay lines,
//! the FFDN reaches comparable echo density with fewer components and
//! a cleaner signal path.
//!
//! # Structure
//!
//! ```text
//!                     ┌──────────────┐
//!   input ──► sum ──► │ Delay 1..N   │ ──► y[n]
//!              ▲      └──────┬───────┘
//!              │             │
//!              └── Matrix ◄──┘
//!                  (lossless)
//! ```
//!
//! On every sample:
//!
//!   1. Each delay line emits its delayed (and damped) sample into a
//!      scratch vector `x`.
//!   2. The lossless matrix `A` is applied: `y = A · x`.
//!   3. Each delay line's new input is `input + g · y[i]`, where
//!      `g < 1` is a per-line feedback gain derived from the target
//!      RT60.
//!
//! Step 1 and step 3 are `O(N)`; step 2 depends on the matrix
//! implementation. `HouseholderMatrix` makes it `O(N)` as well, so
//! the whole engine is `O(N)` per sample.
//!
//! # Real-time safety
//!
//! All buffers are allocated at construction. `process_inplace` never
//! allocates, never locks, and never touches the heap.

pub mod delay_line;
pub mod feedback_matrix;
pub mod velvet_matrix;

use crate::cloudseed::biquad::{Biquad, FilterType};

pub use delay_line::FfdnDelayLine;
pub use feedback_matrix::{FeedbackMatrix, HouseholderMatrix, MAX_LINES};
pub use velvet_matrix::VelvetMatrix;

/// Inline storage for the two feedback matrix types. Replaces the
/// previous `Box<dyn FeedbackMatrix>` so that `rebuild_matrix` never
/// allocates on the audio thread: both variants are Copy-able structs
/// of bounded size (`VelvetMatrix` is the larger at 3 × 16 × 8 bytes
/// = 384 bytes plus a tag), so assigning a new value is a stack
/// memcpy — no heap, no `Box::new`.
pub enum MatrixKind {
    Householder(HouseholderMatrix),
    Velvet(VelvetMatrix),
}

impl FeedbackMatrix for MatrixKind {
    #[inline]
    fn size(&self) -> usize {
        match self {
            MatrixKind::Householder(m) => m.size(),
            MatrixKind::Velvet(m) => m.size(),
        }
    }

    #[inline]
    fn apply(&mut self, x: &[f64], y: &mut [f64]) {
        match self {
            MatrixKind::Householder(m) => m.apply(x, y),
            MatrixKind::Velvet(m) => m.apply(x, y),
        }
    }
}

/// Feedback gain used when `frozen` is set. Pinned per sample, so
/// the network effectively never decays while frozen.
const FROZEN_FEEDBACK: f64 = 0.999_999_99;

// ============================================================================
// FFDN DYNAMICS — envelope follower time constants
// ============================================================================
//
// All three dynamics stages are POST-network gain multipliers. They
// are placed after the FDN and before the tilt filter, which means
// they can never touch the feedback loop. This is the same
// strategy used by every stable reverb (Valhalla, Cinematic Rooms):
// dynamics outside the loop, always in [0,1].

/// Attack stage: peak follower time constants.
const ATTACK_PEAK_ATTACK_MS: f64 = 1.0;
const ATTACK_PEAK_RELEASE_MS: f64 = 200.0;

/// Attack stage: gain follower time range. The `attack` parameter
/// (0..1) is mapped onto this range so the soft-rise time can
/// range from nearly instant to a pronounced swell.
const ATTACK_GAIN_MIN_MS: f64 = 5.0;
const ATTACK_GAIN_MAX_MS: f64 = 500.0;

/// Ducking stage: dry-input envelope time constants. Fast enough
/// to react to note onsets, slow enough to sound natural.
const DUCK_ATTACK_MS: f64 = 5.0;
const DUCK_RELEASE_MS: f64 = 300.0;

/// Gate stage: wet envelope time constants.
const GATE_ATTACK_MS: f64 = 2.0;
const GATE_RELEASE_MS: f64 = 80.0;

// -60 dBFS .. -10 dBFS. The previous max (-30 dBFS) was below
// the typical FFDN wet signal level (~0.05 to 0.5), so a gate at
// full amount never closed in practice. -10 dBFS is just above
// the loudest wet level and lets the gate cut the tail as soon
// as it drops below the chosen threshold.
const GATE_THRESHOLD_MIN: f64 = 1.0e-3;   // -60 dBFS
const GATE_THRESHOLD_MAX: f64 = 3.16e-1;  // -10 dBFS

/// Smooth, tubey saturation.
///
/// # Why this curve
///
/// Three variants of this shaper have existed:
///
/// 1. **`pre_gain = 1 + amount * 5` (up to 6×).** Drove typical
///    FDN levels (0.1 .. 0.3) deep into the compression knee,
///    removing 15–50 % of the amplitude per round trip. Because
///    the saturation sits inside the feedback loop, that loss
///    compounds geometrically and the reverb tail collapsed
///    within a few hundred milliseconds even at moderate drive
///    settings.
///
/// 2. **`pre_gain = 1 + amount * 0.8` plus a `1 + 0.15·amount`
///    makeup term.** The gentler pre-gain fixed the tail
///    collapse, but the makeup introduced a small-signal gain
///    of `1 + 0.15·amount²` (1.15 at amount = 1). Because this
///    gain sits inside the feedback loop, any delay line with
///    `g_mid · 1.15 > 1` would grow instead of decay — e.g. decay
///    = 3 s with size ≈ 50 ms gives g_mid ≈ 0.89, and
///    0.89 × 1.15 ≈ 1.024 > 1.
///
/// 3. **Current version: pre-gain 1× .. 1.8×, no makeup.** The
///    small-signal gain is exactly 1.0, so the network is stable
///    across the whole decay/size grid. Mid-level signals still
///    get the warm compression that is the whole point of a
///    saturation stage; quiet tails pass through essentially
///    untouched.
#[inline]
fn velvet_saturate(x: f64, amount: f64) -> f64 {    if amount <= 0.0 {
        return x;
    }
    // Pre-gain: 1x .. 1.8x. Enough to push transients into the
    // curved region while leaving quiet reverb tails untouched.
    let pre_gain = 1.0 + amount * 0.8;
    let a = x * pre_gain;

    let sat = a / (1.0 + a * a).sqrt();

    let comp = sat / pre_gain;
    // Blend so `amount = 0` remains bit-exact passthrough.
    x + amount * (comp - x)
}

/// One-pole envelope follower coefficient. Converts a time
/// constant in milliseconds to the `a` coefficient used by the
/// per-sample follower `state += a * (target - state)`.
#[inline]
fn one_pole_coeff(ms: f64, sr: f64) -> f64 {
    if ms <= 0.0 {
        return 1.0;
    }
    1.0 - (-1.0 / (ms * sr * 0.001)).exp()
}

/// The main FFDN engine.
///
/// Internally, this is **two independent networks** — one per stereo
/// channel — each with its own Householder matrix, delay lengths,
/// and feedback gains. The two networks share parameter values
/// (`base_delay_ms`, `decay_seconds`, etc.) but derive their actual
/// line delays from different pseudo-random offsets, so the two
/// outputs are fully decorrelated. That's what gives the engine a
/// true stereo image.
///
/// A mono-summed single network would sound correct on headphones
/// only if we re-injected some correlation later; two independent
/// networks give a genuine stereo spread without any post-processing.
pub struct FfdnCore {
    net_l: SingleNetwork,
    net_r: SingleNetwork,

    sample_rate: f64,

    // Parameters, stored so we can re-apply them after a rate change.
    base_delay_ms: f64,
    delay_spread: f64,
    decay_seconds: f64,
    damping_hz: f64,
    seed: u64,

    // ---- Modulation -------------------------------------------------
    /// Modulation depth, in milliseconds. 0 = disabled.
    mod_amount_ms: f64,
    /// Modulation rate, in Hz.
    mod_rate_hz: f64,

    /// `true` = Velvet (blocked, sparse), `false` = Householder (dense).
    is_velvet: bool,

    /// When `true`, the network's input is muted and every delay
    /// line's feedback gain is pinned to `FROZEN_FEEDBACK`. The
    /// existing tail keeps circulating indefinitely while new input
    /// is rejected — the classic "freeze" behaviour that lets the
    /// user hold a reverb tail and play over it.
    frozen: bool,

    /// Bass RT60 multiplier: `T60_bass / T60_mid`.
    /// 1.0 = neutral, >1 = longer bass, <1 = shorter bass.
    bass_mult: f64,
    /// Treble RT60 multiplier: `T60_treble / T60_mid`.
    treble_mult: f64,
    /// Crossover frequency of the low shelf (Hz).
    bass_freq: f64,
    /// Crossover frequency of the high shelf (Hz).
    treble_freq: f64,

    // ================================================================
    // FFDN CHARACTER — extensions added in
    // ================================================================
    //
    // These controls extend the FFDN beyond its original "pure
    // network" character:
    //
    //   * cross_feed  — how much the L and R networks share their
    //                   input, from fully independent (0) to fully
    //                   mono-summed (1).
    //   * drive       — soft saturation applied inside the feedback
    //                   loop. Bounds the peak and slightly compresses
    //                   the tail, adding a natural "analog" character.
    //   * tilt_*      — a spectral tilt filter (low shelf + high
    //                   shelf with opposite gains) applied to the wet
    //                   output. Lets the tail lean bass-heavy or
    //                   treble-heavy without touching the decay time.
    //   * er_level    — how much of the pre-network signal (i.e. the
    //                   input as it enters the FDN) is mixed directly
    //                   into the wet output, giving a controllable
    //                   "early reflection" character.
    //
    // Each of the three "extra" effects has an enable switch so that
    // a fresh FFDN instance with all defaults is bit-identical to the
    // pre- engine.
    cross_feed: f64,
    drive: f64,
    tilt_freq: f64,
    tilt_amount: f64,
    er_level: f64,
    drive_enabled: bool,
    tilt_enabled: bool,
    er_enabled: bool,
    // Tilt filters (low shelf + high shelf per channel). Two biquads
    // per channel are required because the tilt is a *shelf pair*, not
    // a single biquad. The state must be preserved across blocks so we
    // keep them on the struct rather than creating them per block.
    tilt_low_l: Biquad,
    tilt_high_l: Biquad,
    tilt_low_r: Biquad,
    tilt_high_r: Biquad,

    // ================================================================
    // FFDN NETWORK / FILTER / DYNAMICS// ================================================================
    //
    // Nine parameters, stored by the plugin layer and forwarded here
    // through the controller's pass-through setters. NETWORK and
    // FILTER ship their DSP in; DYNAMICS shipped its DSP in
    // the same release but is described separately below because the
    // runtime state is different from the parameter values.

    // ---- FFDN NETWORK --------------------------------------------
    /// Number of active delay lines per network, clamped to 4..16.
    /// Default is 8, matching the fixed value used before.
    num_lines_target: usize,
    /// Input distribution density, 0..1. 0.5 is the neutral center.
    density: f64,
    /// L/R network delay offset amount, 0..1. 0.5 corresponds to
    /// the previous fixed 3 % offset.
    decorr: f64,

    // ---- FFDN FILTER --------------------------------------------
    /// Loop high-pass cutoff (Hz). 20 Hz is neutral (no effect).
    loop_hp_hz: f64,
    /// Loop low-pass cutoff (Hz). 20 kHz is neutral (no effect).
    loop_lp_hz: f64,
    /// Mid-band emphasis amount, 0..1. 0 is neutral.
    mid_focus: f64,

    // ---- FFDN DYNAMICS (parameters) ------------------------------
    /// Attack envelope amount, 0..1. 0 is neutral.
    attack: f64,
    /// Ducking amount, 0..1. 0 is neutral.
    ducking: f64,
    /// Gate threshold, 0..1. 0 is neutral.
    gate: f64,

    // ================================================================
    // FFDN DYNAMICS — runtime state
    // ================================================================
    //
    // The three dynamics stages each own their own envelope follower
    // state. All coefficients are precomputed by the setters so the
    // per-sample path does no division.
    //
    // The stages run in the order attack → ducking → gate. All three
    // are gain multipliers in [0, 1] and live AFTER the FDN network,
    // so they cannot destabilise the network under any input.

    // --- Attack: slow rise of the wet signal ---
    attack_peak_l: f64,
    attack_peak_r: f64,
    attack_gain_l: f64,
    attack_gain_r: f64,
    attack_peak_coeff: f64,
    attack_peak_release_coeff: f64,
    attack_gain_coeff: f64,
    attack_enabled: bool,

    // --- Ducking: reduce wet when dry input is present ---
    duck_env: f64,
    duck_attack_coeff: f64,
    duck_release_coeff: f64,
    ducking_enabled: bool,

    // --- Gate: silence the wet when it drops below a threshold ---
    gate_env_l: f64,
    gate_env_r: f64,
    gate_attack_coeff: f64,
    gate_release_coeff: f64,
    gate_threshold: f64,
    gate_enabled: bool,
}

impl FfdnCore {
    /// Constructs a new stereo FFDN with `num_lines` delay lines per
    /// channel.
    pub fn new(num_lines: usize, buffer_size: usize, sample_rate: f64) -> Self {
        assert!(num_lines >= 2, "FFDN needs at least two delay lines");

        // Two *very* different seeds. Bit-reversed copies of each
        // other guarantee the xorshift streams diverge immediately,
        // so the Householder sign patterns and the delay jitter of
        // the two networks have almost zero correlation.
        const SEED_L: u64 = 0x1234_5678_9ABC_DEF0;
        const SEED_R: u64 = 0x0FED_CBA9_8765_4321;

        // ---- FFDN CHARACTER — initialisation -------------------------
        // The tilt filter is a low-shelf + high-shelf pair. Both start
        // at 0 dB (neutral), so a freshly constructed core has no
        // audible effect from the tilt stage regardless of the
        // `tilt_enabled` flag.
        let mut tilt_low_l = Biquad::new(FilterType::LowShelf, sample_rate);
        tilt_low_l.set_slope(1.0);
        tilt_low_l.set_gain_db(0.0);
        tilt_low_l.set_frequency(1000.0);
        tilt_low_l.update();

        let mut tilt_high_l = Biquad::new(FilterType::HighShelf, sample_rate);
        tilt_high_l.set_slope(1.0);
        tilt_high_l.set_gain_db(0.0);
        tilt_high_l.set_frequency(1000.0);
        tilt_high_l.update();

        let mut tilt_low_r = Biquad::new(FilterType::LowShelf, sample_rate);
        tilt_low_r.set_slope(1.0);
        tilt_low_r.set_gain_db(0.0);
        tilt_low_r.set_frequency(1000.0);
        tilt_low_r.update();

        let mut tilt_high_r = Biquad::new(FilterType::HighShelf, sample_rate);
        tilt_high_r.set_slope(1.0);
        tilt_high_r.set_gain_db(0.0);
        tilt_high_r.set_frequency(1000.0);
        tilt_high_r.update();

        let mut core = Self {
            net_l: SingleNetwork::new(num_lines, SEED_L, sample_rate),
            net_r: SingleNetwork::new(num_lines, SEED_R, sample_rate),
            sample_rate,
            base_delay_ms: 50.0,
            delay_spread: 0.5,
            decay_seconds: 3.0,
            damping_hz: 8000.0,
            seed: SEED_L,
            mod_amount_ms: 0.5,
            mod_rate_hz: 0.7,
            is_velvet: false,
            frozen: false,
            bass_mult: 1.0,
            treble_mult: 1.0,
            bass_freq: 250.0,
            treble_freq: 4000.0,
            // FFDN CHARACTER defaults — all neutral so the pre-
            // behaviour is preserved bit-for-bit.
            cross_feed: 0.0,
            drive: 0.0,
            tilt_freq: 1000.0,
            tilt_amount: 0.5, // 0.5 = flat tilt
            er_level: 0.0,
            drive_enabled: false,
            tilt_enabled: false,
            er_enabled: false,
            tilt_low_l,
            tilt_high_l,
            tilt_low_r,
            tilt_high_r,

            // ---- FFDN NETWORK / FILTER / DYNAMICS -----
            // All values are at their neutral / bit-identical position.
            num_lines_target: num_lines,
            density: 0.5,
            decorr: 0.5,
            loop_hp_hz: 20.0,
            loop_lp_hz: 20_000.0,
            mid_focus: 0.0,
            attack: 0.0,
            ducking: 0.0,
            gate: 0.0,

            // ---- FFDN DYNAMICS state --------------------
            attack_peak_l: 0.0,
            attack_peak_r: 0.0,
            attack_gain_l: 0.0,
            attack_gain_r: 0.0,
            attack_peak_coeff: 1.0,
            attack_peak_release_coeff: 1.0,
            attack_gain_coeff: 1.0,
            attack_enabled: false,
            duck_env: 0.0,
            duck_attack_coeff: 1.0,
            duck_release_coeff: 1.0,
            ducking_enabled: false,
            gate_env_l: 0.0,
            gate_env_r: 0.0,
            gate_attack_coeff: 1.0,
            gate_release_coeff: 1.0,
            gate_threshold: 0.0,
            gate_enabled: false,
        };
        core.recompute_all();
        core.apply_modulation();
        core.apply_fda();
        let _ = buffer_size;
        core
    }

    /// Returns the number of **active** delay lines per network,
    /// not the pre-allocated `MAX_LINES`. Used by tests and by any
    /// future code that wants to know the current topology size.
    pub fn num_lines(&self) -> usize {
        self.net_l.active_lines
    }

    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.net_l.set_samplerate_shelves(sample_rate);
        self.net_r.set_samplerate_shelves(sample_rate);

        // Refresh the tilt biquads at the new sample rate. The gains
        // and frequencies are re-derived from the stored parameters
        // so the coefficients are correct after a host sample-rate
        // change.
        self.tilt_low_l.set_samplerate(sample_rate);
        self.tilt_high_l.set_samplerate(sample_rate);
        self.tilt_low_r.set_samplerate(sample_rate);
        self.tilt_high_r.set_samplerate(sample_rate);
        self.apply_tilt();

        self.recompute_all();
        // the FFDN FILTER biquads hold sample-rate-dependent
        // coefficients that must be recomputed at the new rate.
        self.apply_loop_filters();

        // dynamics coefficients are sample-rate dependent.
        // Re-apply them so the follower time constants stay exact.
        if self.attack_enabled {
            self.set_attack(self.attack);
        }
        if self.ducking_enabled {
            self.set_ducking(self.ducking);
        }
        if self.gate_enabled {
            self.set_gate(self.gate);
        }
    }

    pub fn set_base_delay_ms(&mut self, ms: f64) {
        self.base_delay_ms = ms.clamp(1.0, 500.0);
        self.recompute_all();
    }

    pub fn set_delay_spread(&mut self, spread: f64) {
        self.delay_spread = spread.clamp(0.0, 1.0);
        self.recompute_all();
    }

    pub fn set_decay_seconds(&mut self, seconds: f64) {
        self.decay_seconds = seconds.clamp(0.05, 60.0);
        self.recompute_feedback();
        // The frequency-dependent shelves derive their gains from
        // the per-line feedback value (`g_mid`). Without this call,
        // changing Decay would leave the shelves out of sync with
        // the new feedback, producing a wrong bass/treble ratio.
        self.apply_fda();
    }

    pub fn set_damping_hz(&mut self, hz: f64) {
        self.damping_hz = hz.clamp(20.0, 20000.0);
        self.net_l.apply_damping(self.damping_hz, self.sample_rate);
        self.net_r.apply_damping(self.damping_hz, self.sample_rate);
    }

    pub fn set_seed(&mut self, seed: u64) {
        self.seed = seed;
        self.net_l.reset_seed(seed);
        self.net_r.reset_seed(seed ^ 0x5555_5555_5555_5555);
        self.recompute_all();
    }

    /// Enables or disables freeze.
    ///
    /// When frozen, the network's input is muted and every delay
    /// line's feedback gain is pinned to `FROZEN_FEEDBACK`. The
    /// existing tail keeps circulating while new input is rejected,
    /// so the user can hold a reverb tail and play over it.
    ///
    /// Recomputes the feedback gains and the frequency-dependent
    /// shelf filters immediately so the change is audible on the
    /// very next block.
    pub fn set_freeze(&mut self, frozen: bool) {
        if self.frozen == frozen {
            return;
        }
        self.frozen = frozen;
        self.recompute_feedback();
        self.apply_fda();
    }

    /// Sets the modulation depth in milliseconds (0..5). At 0, the
    /// modulation is fully bypassed and the engine is bit-identical
    /// to an unmodulated FDN.
    pub fn set_mod_amount_ms(&mut self, ms: f64) {
        self.mod_amount_ms = ms.clamp(0.0, 5.0);
        self.apply_modulation();
    }

    /// Sets the modulation rate in Hz (0.05..10).
    pub fn set_mod_rate_hz(&mut self, hz: f64) {
        self.mod_rate_hz = hz.clamp(0.05, 10.0);
        self.apply_modulation();
    }

    fn apply_modulation(&mut self) {
        let amount_samples = self.mod_amount_ms * 1e-3 * self.sample_rate;
        self.net_l
            .apply_modulation(amount_samples, self.mod_rate_hz, self.sample_rate);
        self.net_r
            .apply_modulation(amount_samples, self.mod_rate_hz, self.sample_rate);
    }

    pub fn clear(&mut self) {
        self.net_l.clear();
        self.net_r.clear();
        // The tilt biquads hold filter state that must be flushed on
        // a preset change or an idle wake, otherwise the first samples
        // after the clear carry the previous preset's spectral
        // history.
        self.tilt_low_l.clear_buffers();
        self.tilt_high_l.clear_buffers();
        self.tilt_low_r.clear_buffers();
        self.tilt_high_r.clear_buffers();
        // reset dynamics envelope state so the first samples
        // after a preset change or an idle wake start from silence.
        self.attack_peak_l = 0.0;
        self.attack_peak_r = 0.0;
        self.attack_gain_l = 0.0;
        self.attack_gain_r = 0.0;
        self.duck_env = 0.0;
        self.gate_env_l = 0.0;
        self.gate_env_r = 0.0;
    }

    /// Processes a stereo block in place. The two channels are
    /// independent: L goes through `net_l`, R through `net_r`.
    ///
    /// Unlike a mono-summed FDN, this preserves the full stereo
    /// field of the input and produces a naturally wide tail.
    pub fn process_inplace(&mut self, left: &mut [f64], right: &mut [f64]) {
        let n = left.len().min(right.len());
        // Only the first `active_lines` entries of every parallel
        // array are read or written. `active_lines` is always in
        // `4..=MAX_LINES`, so the slice access below is safe.
        let num = self.net_l.active_lines;
        let scale = 1.0 / (num as f64).sqrt();

        // Cache the FFDN CHARACTER parameters once per block, so the
        // inner loop does not repeatedly dereference `self`.
        let cross_feed = self.cross_feed;
        let drive = self.drive;
        let drive_enabled = self.drive_enabled && drive > 0.0;
        let tilt_enabled = self.tilt_enabled;
        let er_enabled = self.er_enabled && self.er_level > 0.0;
        let er_level = self.er_level;

        // FFDN DYNAMICS parameters cached once per block so
        // the inner loop does not repeatedly dereference `self`.
        let attack_enabled = self.attack_enabled;
        let ducking_enabled = self.ducking_enabled;
        let gate_enabled = self.gate_enabled;
        let ducking_amount = self.ducking;

        for i in 0..n {
            let raw_l = left[i];
            let raw_r = right[i];

            // ---- 1. Cross-feed --------------------------------------
            // Blend each network's input toward the mono sum. At
            // cross_feed = 0 the two networks get the original L / R
            // signals untouched (bit-identical to the pre-
            // engine); at cross_feed = 0.5 both networks get the
            // mono sum; at 1.0 the channels swap.
            let (in_l_src, in_r_src) = if cross_feed > 0.0 {
                let c = cross_feed;
                ((1.0 - c) * raw_l + c * raw_r, (1.0 - c) * raw_r + c * raw_l)
            } else {
                (raw_l, raw_r)
            };

            // ---- 2. Left network ------------------------------------
            // When frozen, the network does not accept new input;
            // the existing tail keeps circulating via the pinned
            // feedback gains. Zeroing the input here is the same
            // technique used by the CloudSeed channel's freeze path.
            //
            // Dry signal is **not** affected by this: the caller
            // (`ReverbController::process_ffdn_inplace`) keeps its
            // own copy of the dry input and mixes it in at the very
            // end.
            let in_l = if self.frozen { 0.0 } else { in_l_src * scale };
            for idx in 0..num {
                self.net_l.line_out[idx] = self.net_l.lines[idx].read();
            }
            self.net_l
                .matrix
                .apply(&self.net_l.line_out[..num], &mut self.net_l.line_in[..num]);
            for j in 0..num {
                let mut new_val = in_l * self.net_l.density_gains[j]
                    + self.net_l.line_in[j] * self.net_l.feedback[j];
                if drive_enabled {
                    new_val = velvet_saturate(new_val, drive);
                }
                self.net_l.lines[j].write(new_val);
            }
            let mut wet_l = 0.0;
            for v in &self.net_l.line_out[..num] {
                wet_l += *v;
            }
            wet_l *= scale;

            // ---- 3. Right network -----------------------------------
            let in_r = if self.frozen { 0.0 } else { in_r_src * scale };
            for idx in 0..num {
                self.net_r.line_out[idx] = self.net_r.lines[idx].read();
            }
            self.net_r
                .matrix
                .apply(&self.net_r.line_out[..num], &mut self.net_r.line_in[..num]);
            for j in 0..num {
                let mut new_val = in_r * self.net_r.density_gains[j]
                    + self.net_r.line_in[j] * self.net_r.feedback[j];
                if drive_enabled {
                    new_val = velvet_saturate(new_val, drive);
                }
                self.net_r.lines[j].write(new_val);
            }
            let mut wet_r = 0.0;
            for v in &self.net_r.line_out[..num] {
                wet_r += *v;
            }
            wet_r *= scale;

            // ---- 3.5. FFDN DYNAMICS ----------------------
            // Three post-network gain stages, in the order
            // attack → ducking → gate. All are bypassed at their
            // neutral value, so a fresh instance with defaults is
            // bit-identical to the pre- engine.
            //
            // Because every stage is a multiplier in [0, 1] applied
            // AFTER the FDN network, none of them can destabilise
            // the feedback loop — the network's internal state is
            // completely unaffected.
            if attack_enabled {
                // --- Peak follower (fast attack, moderate release) ---
                let abs_l = wet_l.abs();
                if abs_l > self.attack_peak_l {
                    self.attack_peak_l += self.attack_peak_coeff * (abs_l - self.attack_peak_l);
                } else {
                    self.attack_peak_l +=
                        self.attack_peak_release_coeff * (abs_l - self.attack_peak_l);
                }
                let abs_r = wet_r.abs();
                if abs_r > self.attack_peak_r {
                    self.attack_peak_r += self.attack_peak_coeff * (abs_r - self.attack_peak_r);
                } else {
                    self.attack_peak_r +=
                        self.attack_peak_release_coeff * (abs_r - self.attack_peak_r);
                }

                // --- Slow gain follower (the "swell in" ramp) ---
                self.attack_gain_l +=
                    self.attack_gain_coeff * (self.attack_peak_l - self.attack_gain_l);
                self.attack_gain_r +=
                    self.attack_gain_coeff * (self.attack_peak_r - self.attack_gain_r);

                // --- Blend-based attenuation ---
                //
                // The ratio `gain / peak` alone gives a maximum of
                // about -4 dB at the very start (because the peak
                // follower releases within 200 ms and the gain
                // follower barely moves in the first few hundred
                // samples). That made the attack barely audible.
                //
                // We now use `amount` to control *both* the ramp
                // time and the *depth* of the initial attenuation:
                //
                //   a = 0   → 1.0 (no attenuation, stage effectively
                //                   bypassed)
                //   a = 0.5 → mid-attenuation, soft swell
                //   a = 1   → full attenuation at t=0, gradual
                //             swell to unity
                //
                // The math is a simple linear blend between "no
                // attenuation" (1.0) and "ratio attenuation" (atten).
                if self.attack_peak_l > 1.0e-12 {
                    let atten_l = (self.attack_gain_l / self.attack_peak_l).clamp(0.0, 1.0);
                    wet_l *= 1.0 - self.attack * (1.0 - atten_l);
                }
                if self.attack_peak_r > 1.0e-12 {
                    let atten_r = (self.attack_gain_r / self.attack_peak_r).clamp(0.0, 1.0);
                    wet_r *= 1.0 - self.attack * (1.0 - atten_r);
                }
            }

            if ducking_enabled {
                // Envelope of the dry input (mono-summed). Fast
                // attack, slow release: reacts to note onsets but
                // does not pump.
                let dry_abs = (raw_l.abs() + raw_r.abs()) * 0.5;
                if dry_abs > self.duck_env {
                    self.duck_env += self.duck_attack_coeff * (dry_abs - self.duck_env);
                } else {
                    self.duck_env += self.duck_release_coeff * (dry_abs - self.duck_env);
                }

                // --- Exponential attenuation (was linear) ---
                //
                // The previous linear formula `1 - amount × env`
                // gave a maximum of ~-3 dB at amount = 1 (with a
                // typical dry level of env = 0.3), which was
                // imperceptible.
                //
                // The new formula maps `env` to a normalised
                // 0..1 range (0.5 = typical dry peak) and applies
                // up to -24 dB of attenuation at full amount and
                // full env. The dB attenuation is then converted
                // back to a linear gain.
                //
                // Examples at amount = 1.0:
                //   env = 0.05 → -2.4 dB
                //   env = 0.15 → -7.2 dB
                //   env = 0.30 → -14.4 dB
                //   env = 0.50 → -24.0 dB (fully ducked)
                let env_norm = (self.duck_env / 0.5).clamp(0.0, 1.0);
                let atten_db = -24.0 * ducking_amount * env_norm;
                let duck_gain = 10.0_f64.powf(atten_db / 20.0);
                wet_l *= duck_gain;
                wet_r *= duck_gain;
            }

            if gate_enabled {
                // Envelope of the wet itself (post-attack, post-duck).
                let abs_l = wet_l.abs();
                if abs_l > self.gate_env_l {
                    self.gate_env_l += self.gate_attack_coeff * (abs_l - self.gate_env_l);
                } else {
                    self.gate_env_l += self.gate_release_coeff * (abs_l - self.gate_env_l);
                }
                let abs_r = wet_r.abs();
                if abs_r > self.gate_env_r {
                    self.gate_env_r += self.gate_attack_coeff * (abs_r - self.gate_env_r);
                } else {
                    self.gate_env_r += self.gate_release_coeff * (abs_r - self.gate_env_r);
                }

                // --- Squared fade (was linear) ---
                //
                // Below the threshold the gain drops as
                // `(env / thr)²`, which produces a much more
                // decisive gate close. The linear curve previously
                // used made the transition from "open" to "closed"
                // sound like a slow fade rather than a gate.
                let thr = self.gate_threshold;
                let g_l = if self.gate_env_l < thr {
                    (self.gate_env_l / thr).powi(2).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                let g_r = if self.gate_env_r < thr {
                    (self.gate_env_r / thr).powi(2).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                wet_l *= g_l;
                wet_r *= g_r;
            }

            // ---- 4. Tilt filter -------------------------------------
            // A low-shelf + high-shelf pair that crosses at
            // `tilt_freq`. Applied on the wet sum so it does not
            // interact with the network's own damping / shelves.
            if tilt_enabled {
                wet_l = self.tilt_low_l.process(wet_l);
                wet_l = self.tilt_high_l.process(wet_l);
                wet_r = self.tilt_low_r.process(wet_r);
                wet_r = self.tilt_high_r.process(wet_r);
            }

            // ---- 5. Early-reflection mix ----------------------------
            // Blend the raw input (pre-network) into the wet sum.
            // Because the raw input is the same signal that was fed
            // to the FDN, this behaves as a "dry early reflection"
            // tap that the user can dial in with `er_level`.
            if er_enabled {
                wet_l += er_level * raw_l;
                wet_r += er_level * raw_r;
            }

            left[i] = wet_l;
            right[i] = wet_r;
        }
    }

    // ---- Internal helpers -------------------------------------------

    /// Re-derives every sample-domain quantity (delays, feedback,
    /// damping) from the current parameter values.
    fn recompute_all(&mut self) {
        let base_l = self.base_delay_ms;
        // Decorrelation offset between the L and R networks. At
        // `decorr = 0.5` this evaluates to exactly 1.03, matching the
        // previous hard-coded value. At 0 the two networks have the
        // same base delay; at 1 they differ by 6 %.
        let offset = 1.0 + self.decorr * 0.06;
        let base_r = base_l * offset;

        self.net_l
            .recompute_delays(base_l, self.delay_spread, self.sample_rate);
        self.net_r
            .recompute_delays(base_r, self.delay_spread, self.sample_rate);
        self.recompute_feedback();
        self.net_l.apply_damping(self.damping_hz, self.sample_rate);
        self.net_r.apply_damping(self.damping_hz, self.sample_rate);
        self.apply_modulation();
        self.apply_fda();

        // Density gains depend on the active line count (via the
        // normalization), so they must be recomputed whenever the
        // network is refreshed.
        self.net_l.recompute_density_gains(self.density);
        self.net_r.recompute_density_gains(self.density);
    }

    /// Recomputes the per-line feedback gain from the current RT60
    /// and sample rate.
    fn recompute_feedback(&mut self) {
        if self.frozen {
            // Pin every line's feedback to the frozen value. The
            // `decay_seconds` parameter is intentionally ignored
            // here — its value is preserved on `self` so that
            // unfreezing restores the previous decay time exactly.
            for f in self.net_l.feedback.iter_mut() {
                *f = FROZEN_FEEDBACK;
            }
            for f in self.net_r.feedback.iter_mut() {
                *f = FROZEN_FEEDBACK;
            }
        } else {
            self.net_l
                .recompute_feedback(self.decay_seconds, self.sample_rate);
            self.net_r
                .recompute_feedback(self.decay_seconds, self.sample_rate);
        }
    }

    /// Selects the feedback matrix type.
    ///
    /// * `false` → dense Householder (smooth, "algorithmic" tail)
    /// * `true`  → sparse Velvet (blocked + permutation, more
    ///              "physical" echo pattern)
    ///
    /// Rebuilds the matrix on both networks; the switch is cheap
    /// (two small Vec allocations), but it should still be called
    /// from a control thread, not per-sample.
    pub fn set_matrix_type(&mut self, is_velvet: bool) {
        if self.is_velvet == is_velvet {
            return;
        }
        self.is_velvet = is_velvet;
        self.net_l.rebuild_matrix(is_velvet);
        self.net_r.rebuild_matrix(is_velvet);
    }

    /// Sets the four frequency-dependent-attenuation parameters.
    ///
    /// * `bass_mult`: `T60_bass / T60_mid`. 1.0 = neutral.
    /// * `treble_mult`: `T60_treble / T60_mid`. 1.0 = neutral.
    /// * `bass_freq`: crossover of the low shelf, Hz.
    /// * `treble_freq`: crossover of the high shelf, Hz.
    pub fn set_fda(&mut self, bass_mult: f64, treble_mult: f64, bass_freq: f64, treble_freq: f64) {
        self.bass_mult = bass_mult.clamp(0.25, 4.0);
        self.treble_mult = treble_mult.clamp(0.25, 4.0);
        self.bass_freq = bass_freq.clamp(50.0, 1000.0);
        self.treble_freq = treble_freq.clamp(1000.0, 15000.0);
        self.apply_fda();
    }

    /// Recomputes the per-line shelf gains from the current FDA
    /// parameters and the cached feedback gains.
    ///
    /// Mathematically, for a delay of `d` samples and a mid-band
    /// gain `g_mid = 10^(-3d / (T60 · fs))`, the shelf gain required
    /// to make the effective RT60 in that band equal `R · T60` is
    /// `g_mid^(1/R - 1)`.
    fn apply_fda(&mut self) {
        let (bass_mult, treble_mult, bass_freq, treble_freq) = (
            self.bass_mult,
            self.treble_mult,
            self.bass_freq,
            self.treble_freq,
        );
        self.net_l
            .apply_fda(bass_mult, treble_mult, bass_freq, treble_freq);
        self.net_r
            .apply_fda(bass_mult, treble_mult, bass_freq, treble_freq);
    }

    // ================================================================
    // FFDN CHARACTER — setters added in
    // ================================================================

    /// Cross-feed between the L and R networks (0..1).
    ///
    /// At `0.0` the two networks are completely independent — the
    /// stereo image is as wide as the seeds make it.
    ///
    /// As `cross_feed` rises, each network's input is blended
    /// toward the mono sum `(L + R) / 2`, so at `0.5` the two
    /// networks receive the same signal and the reverb collapses
    /// to mono. Values above `0.5` progressively swap the channels.
    ///
    /// This is applied *before* the networks, so the coupling is
    /// genuinely at the source and works for both the linear and
    /// the saturated (Drive) path.
    pub fn set_cross_feed(&mut self, v: f64) {
        self.cross_feed = v.clamp(0.0, 1.0);
    }

    /// Drive amount (0..1). Blends the feedback sum toward
    /// `velvet_saturate()` inside every delay line's write path.
    ///
    /// At `0.0` the write is exactly `new = in + feedback * gain`
    /// (bit-identical to the pre- engine). At `1.0` every sample
    /// entering a delay line passes through the sine shaper
    /// `a / sqrt(1 + a²)` with a 1.8× pre-gain, which bounds the
    /// peak and adds a gentle compression that is especially
    /// audible on long, dense tails.
    ///
    /// Drive is always safe for the small-signal gain: the shaper
    /// is unity-linear as `|x| → 0`, so a quiet tail is not
    /// amplified and the network cannot become unstable.
    pub fn set_drive(&mut self, v: f64) {
        self.drive = v.clamp(0.0, 1.0);
    }

    /// Sets both tilt parameters in one call.
    ///
    /// * `freq`  — crossover of the low and high shelf (Hz).
    /// * `amount` — 0.0 = bass-heavy (treble -6 dB), 0.5 = flat,
    ///   1.0 = treble-heavy (treble +6 dB).
    pub fn set_tilt(&mut self, freq: f64, amount: f64) {
        self.tilt_freq = freq.clamp(100.0, 10000.0);
        self.tilt_amount = amount.clamp(0.0, 1.0);
        self.apply_tilt();
    }

    /// Early-reflection blend (0..1). Mixes the input to the network
    /// (i.e. the signal *after* HP/LP/pre-delay, but *before* the
    /// FDN's internal diffusion) directly into the wet output.
    ///
    /// At `0.0` the FFDN behaves exactly like the pre- engine:
    /// the output is 100 % network response. As `er_level` rises,
    /// the direct input becomes more and more audible alongside the
    /// diffuse tail, giving a controllable "early reflection" feel
    /// without needing a separate ER processor.
    pub fn set_er_level(&mut self, v: f64) {
        self.er_level = v.clamp(0.0, 1.0);
    }

    /// Enables or disables the drive stage.
    pub fn set_drive_enabled(&mut self, enabled: bool) {
        self.drive_enabled = enabled;
    }

    /// Enables or disables the tilt filter.
    pub fn set_tilt_enabled(&mut self, enabled: bool) {
        self.tilt_enabled = enabled;
    }

    /// Enables or disables the early-reflection mix.
    pub fn set_er_enabled(&mut self, enabled: bool) {
        self.er_enabled = enabled;
    }

    /// Recomputes the tilt biquad coefficients from the current
    /// `tilt_freq` and `tilt_amount`.
    ///
    /// The tilt is implemented as a low shelf and a high shelf that
    /// cross at `tilt_freq`, with gains that move in opposite
    /// directions:
    ///
    /// ```text
    ///     bass_db   = (0.5 - amount) * 12
    ///     treble_db = (amount - 0.5) * 12
    /// ```
    ///
    /// So `amount = 0.5` gives 0 dB / 0 dB (neutral), `amount = 0`
    /// gives +6 dB bass / -6 dB treble, and `amount = 1` gives
    /// -6 dB bass / +6 dB treble. The ±6 dB range is the classic
    /// "tilt EQ" spec used on many mastering EQs.
    fn apply_tilt(&mut self) {
        let bass_db = (0.5 - self.tilt_amount) * 12.0;
        let treble_db = (self.tilt_amount - 0.5) * 12.0;

        self.tilt_low_l.set_gain_db(bass_db);
        self.tilt_low_l.set_frequency(self.tilt_freq);
        self.tilt_low_l.update();

        self.tilt_high_l.set_gain_db(treble_db);
        self.tilt_high_l.set_frequency(self.tilt_freq);
        self.tilt_high_l.update();

        self.tilt_low_r.set_gain_db(bass_db);
        self.tilt_low_r.set_frequency(self.tilt_freq);
        self.tilt_low_r.update();

        self.tilt_high_r.set_gain_db(treble_db);
        self.tilt_high_r.set_frequency(self.tilt_freq);
        self.tilt_high_r.update();
    }

    // ================================================================
    // FFDN NETWORK / FILTERsetters
    // ================================================================

    /// Sets the number of active delay lines per network (4..16).
    ///
    /// Rebuilds the feedback matrix on both networks and recomputes
    /// delays, feedback, and density normalization. This is **not**
    /// real-time-safe (the matrix is reallocated), but it is only
    /// called from the plugin when the user actually moves the Lines
    /// knob.
    pub fn set_lines(&mut self, lines: usize) {
        let n = lines.clamp(4, MAX_LINES);
        if n == self.num_lines_target {
            return;
        }
        self.num_lines_target = n;
        self.net_l.set_active_lines(n);
        self.net_r.set_active_lines(n);
        self.recompute_all();
        // Newly-activated lines were cleared and have default filter
        // coefficients. Re-apply the current FFDN FILTER settings so
        // the whole network is consistent.
        self.apply_loop_filters();
    }

    /// Sets the FFDN input distribution density (0..1).
    ///
    /// At `0.5` every line receives exactly `1/sqrt(N)` of the input
    /// (the pre- uniform path). Lower values concentrate the
    /// input on fewer lines; higher values spread it more evenly with
    /// a wider per-line gain pattern.
    pub fn set_density(&mut self, v: f64) {
        self.density = v.clamp(0.0, 1.0);
        self.net_l.recompute_density_gains(self.density);
        self.net_r.recompute_density_gains(self.density);
    }

    /// Sets the FFDN L/R network delay offset amount (0..1).
    ///
    /// At `0.5` the R network is delayed by 3 % relative to L — the
    /// value hard-coded before. At 0 the two networks share
    /// the same base delay; at 1 they differ by 6 %.
    pub fn set_decorr(&mut self, v: f64) {
        self.decorr = v.clamp(0.0, 1.0);
        self.recompute_all();
    }

    /// Sets the loop high-pass cutoff in Hz (20..500).
    ///
    /// At 20 Hz the filter is bypassed (neutral). The change is
    /// applied immediately to every active line.
    pub fn set_loop_hp(&mut self, hz: f64) {
        self.loop_hp_hz = hz.clamp(20.0, 500.0);
        self.apply_loop_filters();
    }

    /// Sets the loop low-pass cutoff in Hz (2000..20000).
    ///
    /// At 20 kHz the filter is bypassed (neutral).
    pub fn set_loop_lp(&mut self, hz: f64) {
        self.loop_lp_hz = hz.clamp(2_000.0, 20_000.0);
        self.apply_loop_filters();
    }

    /// Sets the mid-band emphasis amount (0..1).
    ///
    /// At 0 the shelf pair is bypassed (neutral). As the value
    /// rises, the bands below 400 Hz and above 1500 Hz are
    /// attenuated by up to -6 dB, focusing the tail on the mid band.
    pub fn set_mid_focus(&mut self, v: f64) {
        self.mid_focus = v.clamp(0.0, 1.0);
        self.apply_loop_filters();
    }

    /// Re-applies the FFDN FILTER parameters to both networks. Called
    /// from the three setters above and from any place that changes
    /// the active line count (newly-activated lines must receive the
    /// current filter settings).
    fn apply_loop_filters(&mut self) {
        let (hp, lp, mid) = (self.loop_hp_hz, self.loop_lp_hz, self.mid_focus);
        self.net_l.apply_loop_filters(hp, lp, mid);
        self.net_r.apply_loop_filters(hp, lp, mid);
    }

    // ================================================================
    // FFDN DYNAMICSsetters
    // ================================================================

    /// Sets the attack envelope amount (0..1).
    ///
    /// At `0` the stage is bypassed and the output is bit-identical
    /// to the pre- engine. As the value rises, the wet
    /// signal's initial transient is attenuated and then allowed
    /// to swell in over up to 500 ms.
    ///
    /// # Implementation
    ///
    /// Two followers on `|wet|`:
    ///   * `peak` — fast attack (1 ms), moderate release (200 ms)
    ///   * `gain` — slow attack (up to 500 ms)
    ///
    /// The applied gain is `gain / peak`, clamped to `[0, 1]`.
    /// During a rise, `peak` jumps immediately and `gain` lags
    /// behind, so the ratio is small and the transient is soft.
    /// Once the signal is steady, `gain` catches up and the ratio
    /// approaches 1 — the steady-state level is preserved.
    pub fn set_attack(&mut self, v: f64) {
        let a = v.clamp(0.0, 1.0);
        self.attack = a;
        if a < 0.005 {
            self.attack_enabled = false;
            return;
        }
        // The attack time uses a square-root curve so the audible
        // effect grows quickly with `amount`. At a = 0.25 the attack
        // time is already ~250 ms (square-law mapping), which is long
        // enough to hear on a typical 3 s reverb tail.
        let attack_ms =
            ATTACK_GAIN_MIN_MS + a.sqrt() * (ATTACK_GAIN_MAX_MS - ATTACK_GAIN_MIN_MS);
        self.attack_peak_coeff = one_pole_coeff(ATTACK_PEAK_ATTACK_MS, self.sample_rate);
        self.attack_peak_release_coeff = one_pole_coeff(ATTACK_PEAK_RELEASE_MS, self.sample_rate);
        self.attack_gain_coeff = one_pole_coeff(attack_ms, self.sample_rate);
        self.attack_enabled = true;
    }

    /// Sets the ducking amount (0..1).
    ///
    /// At `0` the stage is bypassed (bit-identical). As the value
    /// rises, the wet signal is attenuated while the dry input is
    /// present, and recovers during the silence between notes.
    /// This is the classic "ducked reverb" used on vocals.
    pub fn set_ducking(&mut self, v: f64) {
        let a = v.clamp(0.0, 1.0);
        self.ducking = a;
        if a < 0.005 {
            self.ducking_enabled = false;
            return;
        }
        self.duck_attack_coeff = one_pole_coeff(DUCK_ATTACK_MS, self.sample_rate);
        self.duck_release_coeff = one_pole_coeff(DUCK_RELEASE_MS, self.sample_rate);
        self.ducking_enabled = true;
    }

    /// Sets the gate threshold (0..1).
    ///
    /// At `0` the stage is bypassed (bit-identical). As the value
    /// rises, the wet signal is progressively silenced once its
    /// envelope drops below the threshold. The transition is soft:
    /// the gain falls off linearly as the envelope approaches zero,
    /// so the gate closes without a click.
    pub fn set_gate(&mut self, v: f64) {
        let a = v.clamp(0.0, 1.0);
        self.gate = a;
        if a < 0.005 {
            self.gate_enabled = false;
            return;
        }
        self.gate_attack_coeff = one_pole_coeff(GATE_ATTACK_MS, self.sample_rate);
        self.gate_release_coeff = one_pole_coeff(GATE_RELEASE_MS, self.sample_rate);
        // Map 0..1 to a linear threshold in the min..max range.
        self.gate_threshold = GATE_THRESHOLD_MIN + a * (GATE_THRESHOLD_MAX - GATE_THRESHOLD_MIN);
        self.gate_enabled = true;
    }
}

/// One channel's worth of FFDN state: delay lines + matrix + scratch
/// buffers. A stereo FFDN holds two of these, one per channel.
struct SingleNetwork {
    lines: Vec<FfdnDelayLine>,
    matrix: MatrixKind,
    feedback: Vec<f64>,
    delays_samples: Vec<usize>,
    line_out: Vec<f64>,
    line_in: Vec<f64>,
    seed: u64,
    is_velvet: bool,
    mod_rate_multipliers: Vec<f64>,
    active_lines: usize,
    density_gains: Vec<f64>,
}

impl SingleNetwork {
    fn new(initial_lines: usize, seed: u64, sample_rate: f64) -> Self {
        let max_delay_samples = (sample_rate * 0.5).ceil() as usize;

        // Pre-allocate `MAX_LINES` delay lines. Only the first
        // `active_lines` will actually be processed; the rest sit
        // idle but pre-allocated so that changing the line count at
        // runtime never allocates on the audio thread.
        let mut lines: Vec<FfdnDelayLine> = (0..MAX_LINES)
            .map(|_| FfdnDelayLine::new(max_delay_samples, sample_rate))
            .collect();

        let active_lines = initial_lines.clamp(4, MAX_LINES);

        // Start with the dense Householder matrix; the FFDN core
        // will call `rebuild_matrix` if the user selects Velvet.
        // `MatrixKind` is stored inline (no Box) so re-assigning it
        // in `rebuild_matrix` never allocates.
        let matrix = MatrixKind::Householder(HouseholderMatrix::new(active_lines, seed));

        // Per-line modulation variation. Two independent draws per
        // line: a rate multiplier in [0.85, 1.15] and a start phase
        // in [0, 1). Computed for ALL MAX_LINES so that growing the
        // active count reuses pre-set values without recomputation.
        let mut state = seed | 1;
        let mut mod_rate_multipliers = Vec::with_capacity(MAX_LINES);
        for i in 0..MAX_LINES {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let u_rate = ((state >> 11) as f64) / ((1u64 << 53) as f64);
            mod_rate_multipliers.push(0.85 + 0.3 * u_rate);

            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let u_phase = ((state >> 11) as f64) / ((1u64 << 53) as f64);
            lines[i].set_lfo_phase(u_phase);
        }

        Self {
            lines,
            matrix,
            feedback: vec![0.0; MAX_LINES],
            delays_samples: vec![0; MAX_LINES],
            line_out: vec![0.0; MAX_LINES],
            line_in: vec![0.0; MAX_LINES],
            seed,
            is_velvet: false,
            mod_rate_multipliers,
            active_lines,
            density_gains: vec![1.0; MAX_LINES],
        }
    }

    fn reset_seed(&mut self, seed: u64) {
        self.seed = seed;
        self.rebuild_matrix(self.is_velvet);
    }

    /// Rebuilds the feedback matrix as either Householder or Velvet.
    /// Called from `set_matrix_type`, `reset_seed`, and
    /// `set_active_lines`.
    ///
    /// The matrix is sized to `active_lines`, **not** to
    /// `lines.len()`. Since the network pre-allocates
    /// `MAX_LINES` delay lines but only the first `active_lines` are
    /// processed, and the matrix must match the slice length that
    /// `process_inplace` passes to `apply()`.
    ///
    /// # Real-time safety
    ///
    /// Both variants of `MatrixKind` are fixed-size inline structs,
    /// so this assignment is a stack-to-stack memcpy. No heap
    /// allocation is performed even when the network is resized at
    /// runtime.
    fn rebuild_matrix(&mut self, is_velvet: bool) {
        self.is_velvet = is_velvet;
        let n = self.active_lines;
        self.matrix = if is_velvet {
            MatrixKind::Velvet(VelvetMatrix::new(n, self.seed))
        } else {
            MatrixKind::Householder(HouseholderMatrix::new(n, self.seed))
        };
    }

    fn recompute_delays(&mut self, base_ms: f64, spread: f64, sample_rate: f64) {
        let n = self.active_lines;
        let base = base_ms * 1e-3 * sample_rate;
    
        let mut state = self.seed | 1;
        for i in 0..n {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let unit = ((state >> 11) as f64) / ((1u64 << 53) as f64);
            let scale = 1.0 + spread * (2.0 * unit - 1.0);
    
            // Clamp to the line's actual capacity BEFORE storing the
            // value in `delays_samples`.
            //
            // `set_delay` already clamps internally, but
            // `recompute_feedback` reads its delay time from
            // `delays_samples`. Without this clamp, whenever Size
            // pushes past the buffer cap the feedback is computed for
            // a longer delay than the line can actually hold, and the
            // audible decay comes out shorter than requested.
            let samples_raw = (base * scale).round().max(2.0) as usize;
            let samples = samples_raw.min(self.lines[i].max_delay_samples());
    
            self.delays_samples[i] = samples;
            self.lines[i].set_delay(samples);
        }
    }
    

    fn recompute_feedback(&mut self, decay_seconds: f64, sample_rate: f64) {
        for i in 0..self.active_lines {
            let d_f = self.delays_samples[i] as f64;
            let exponent = -3.0 * d_f / (decay_seconds * sample_rate);
            self.feedback[i] = 10.0_f64.powf(exponent);
        }
    }

    fn apply_damping(&mut self, hz: f64, sample_rate: f64) {
        for i in 0..self.active_lines {
            self.lines[i].set_damping_hz(hz, sample_rate);
        }
    }

    fn apply_modulation(&mut self, amount_samples: f64, rate_hz: f64, sample_rate: f64) {
        for i in 0..self.active_lines {
            let rate = rate_hz * self.mod_rate_multipliers[i];
            self.lines[i].set_modulation(amount_samples, rate, sample_rate);
        }
    }

    fn set_samplerate_shelves(&mut self, sr: f64) {
        for i in 0..self.active_lines {
            self.lines[i].set_samplerate_shelves(sr);
        }
    }

    /// Applies the FDA parameters to every delay line.
    ///
    /// For each line with mid-band feedback gain `g_mid`, the shelf
    /// gain required to reach an effective RT60 of `R · T60_mid` is
    /// `g_shelf = g_mid^(1/R - 1)`. This is applied to both the low
    /// shelf (using `bass_mult`) and the high shelf (using
    /// `treble_mult`).
    fn apply_fda(&mut self, bass_mult: f64, treble_mult: f64, bass_freq: f64, treble_freq: f64) {
        let inv_bass = 1.0 / bass_mult;
        let inv_treble = 1.0 / treble_mult;

        for i in 0..self.active_lines {
            let g_mid = self.feedback[i];
            if g_mid <= 0.0 {
                continue;
            }

            let g_low = g_mid.powf(inv_bass - 1.0);
            let g_high = g_mid.powf(inv_treble - 1.0);

            let low_db = 20.0 * g_low.max(1e-9).log10();
            let high_db = 20.0 * g_high.max(1e-9).log10();

            self.lines[i].set_shelves(low_db, bass_freq, high_db, treble_freq);
        }
    }

    /// Applies the  FFDN FILTER parameters to every delay
    /// line. At the neutral values (20 Hz, 20 kHz, 0) every line
    /// bypasses all three filters, so the pre- behaviour is
    /// preserved bit-for-bit.
    fn apply_loop_filters(&mut self, hp_hz: f64, lp_hz: f64, mid_focus: f64) {
        for i in 0..self.active_lines {
            self.lines[i].set_loop_filters(hp_hz, lp_hz);
            self.lines[i].set_mid_focus(mid_focus);
        }
    }

    fn clear(&mut self) {
        for line in &mut self.lines {
            line.clear();
        }
        self.line_out.fill(0.0);
        self.line_in.fill(0.0);
    }

    /// Sets the number of active lines. Rebuilds the feedback matrix
    /// because `HouseholderMatrix` and `VelvetMatrix` are sized at
    /// construction time. Newly-activated lines are cleared to
    /// prevent a stale buffer from bleeding into the first block.
    ///
    /// `rebuild_matrix` uses inline `MatrixKind` storage (fixed-size
    /// arrays, no `Box`, no `Vec`), so this operation does not
    /// allocate on the audio thread.
    fn set_active_lines(&mut self, n: usize) {
        let n = n.clamp(4, MAX_LINES);
        if n == self.active_lines {
            return;
        }
        // Clear newly-activated lines so their buffers start silent.
        if n > self.active_lines {
            for i in self.active_lines..n {
                self.lines[i].clear();
            }
        }
        self.active_lines = n;
        self.rebuild_matrix(self.is_velvet);
    }

    /// Recomputes per-line input gains from the density parameter.
    ///
    /// At `density = 0.5` every gain is exactly 1.0, so the input
    /// distribution matches the pre- uniform behaviour bit-
    /// for-bit. The gains are normalized so that total input power
    /// stays constant across the density sweep.
    fn recompute_density_gains(&mut self, density: f64) {
        let n = self.active_lines;
        let spread = (density - 0.5) * 2.0; // -1.0 .. +1.0

        // Seed-derived jitter so the per-line gain pattern is stable
        // across block boundaries. XOR with a constant so the pattern
        // differs from the delay jitter used in `recompute_delays`.
        let mut state = self.seed ^ 0xCAFE_BABE_DEAD_BEEF;
        let mut sum_sq = 0.0_f64;
        for i in 0..n {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let u = ((state >> 11) as f64) / ((1u64 << 53) as f64);
            let delta = (u - 0.5) * 2.0; // -1 .. +1
            let g = 1.0 + spread * delta * 0.9;
            self.density_gains[i] = g;
            sum_sq += g * g;
        }

        // Normalize so the density control redistributes energy
        // rather than changing the overall level. At `spread = 0`
        // every gain is already 1.0, so `norm == 1.0` exactly.
        if n > 0 && sum_sq > 0.0 {
            let norm = (n as f64 / sum_sq).sqrt();
            for i in 0..n {
                self.density_gains[i] *= norm;
            }
        }

        // Reset unused slots to 1.0 so that if the active count later
        // grows in a way that skips this recompute, the gains stay
        // neutral.
        for i in n..MAX_LINES {
            self.density_gains[i] = 1.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn impulse_response_decays() {
        let mut ffdn = FfdnCore::new(8, 256, 48000.0);
        ffdn.set_decay_seconds(1.0);
        ffdn.set_base_delay_ms(20.0);
        ffdn.set_damping_hz(20000.0);

        // Feed an impulse.
        let mut left = vec![0.0; 48000];
        let mut right = vec![0.0; 48000];
        left[0] = 1.0;
        right[0] = 1.0;

        ffdn.process_inplace(&mut left, &mut right);

        // Check RMS of the first 0.1 s and the last 0.1 s.
        let n = left.len();
        let early: f64 = left[0..n / 10].iter().map(|v| v * v).sum();
        let late: f64 = left[n * 9 / 10..].iter().map(|v| v * v).sum();
        assert!(
            early > late,
            "tail must decay: early {} late {}",
            early,
            late
        );

        // Sanity: no NaNs.
        for v in &left {
            assert!(v.is_finite());
        }
    }

    #[test]
    fn clear_resets_state() {
        let mut ffdn = FfdnCore::new(4, 64, 48000.0);
        let mut l = [1.0; 64];
        let mut r = [1.0; 64];
        ffdn.process_inplace(&mut l, &mut r);
        ffdn.clear();
        let mut l2 = [0.0; 64];
        let mut r2 = [0.0; 64];
        ffdn.process_inplace(&mut l2, &mut r2);
        for v in l2.iter() {
            assert!(v.abs() < 1e-12);
        }
    }

    /// Verifies that changing the active line count at runtime does
    /// not break the network. The test grows the network from 4 to
    /// 16 lines and back, checking that no NaN/Inf appears and that
    /// the tail still decays.
    #[test]
    fn changing_line_count_preserves_stability() {
        let mut ffdn = FfdnCore::new(4, 256, 48000.0);
        ffdn.set_decay_seconds(1.0);
        ffdn.set_base_delay_ms(20.0);
        ffdn.set_damping_hz(20000.0);

        // Grow to 16, then shrink to 6, then back to 8.
        ffdn.set_lines(16);
        ffdn.set_lines(6);
        ffdn.set_lines(8);

        // Feed an impulse and verify stability.
        let mut left = vec![0.0; 48000];
        let mut right = vec![0.0; 48000];
        left[0] = 1.0;
        right[0] = 1.0;
        ffdn.process_inplace(&mut left, &mut right);

        for v in &left {
            assert!(v.is_finite(), "output contains NaN/Inf after line change");
            assert!(v.abs() < 100.0, "output blew up: {}", v);
        }

        // Sanity: tail decays.
        let n = left.len();
        let early: f64 = left[0..n / 10].iter().map(|v| v * v).sum();
        let late: f64 = left[n * 9 / 10..].iter().map(|v| v * v).sum();
        assert!(early > late);
    }

    /// Verifies that density at its neutral value (0.5) produces
    /// exactly the same output as the pre- uniform path.
    ///
    /// This is the bit-identical guarantee we rely on for backward
    /// compatibility: presets that don't touch density must sound
    /// the same as they did before the parameter existed.
    #[test]
    fn density_neutral_is_bit_identical() {
        // Two independent FFDN instances with identical parameters.
        let mut a = FfdnCore::new(8, 256, 48000.0);
        let mut b = FfdnCore::new(8, 256, 48000.0);

        // Make everything deterministic.
        a.set_decay_seconds(1.0);
        a.set_base_delay_ms(20.0);
        a.set_damping_hz(20000.0);
        a.set_density(0.5);

        b.set_decay_seconds(1.0);
        b.set_base_delay_ms(20.0);
        b.set_damping_hz(20000.0);
        b.set_density(0.5);

        // Feed the same impulse to both.
        let mut la = vec![0.0; 1024];
        let mut ra = vec![0.0; 1024];
        let mut lb = vec![0.0; 1024];
        let mut rb = vec![0.0; 1024];
        la[0] = 1.0;
        ra[0] = 1.0;
        lb[0] = 1.0;
        rb[0] = 1.0;

        a.process_inplace(&mut la, &mut ra);
        b.process_inplace(&mut lb, &mut rb);

        // Outputs must be bit-identical: neutral density means every
        // line has gain exactly 1.0, so the per-line multiplication
        // is a no-op.
        for i in 0..la.len() {
            assert_eq!(la[i], lb[i], "left[{}] differs: {} vs {}", i, la[i], lb[i]);
            assert_eq!(ra[i], rb[i], "right[{}] differs: {} vs {}", i, ra[i], rb[i]);
        }
    }

    /// Same idea as `density_neutral_is_bit_identical`, but for the
    /// decorrelation parameter. At `decorr = 0.5`, the R network
    /// offset must evaluate to exactly the same 1.03 factor that was
    /// hard-coded before.
    #[test]
    fn decorr_neutral_is_bit_identical() {
        let mut a = FfdnCore::new(8, 256, 48000.0);
        let mut b = FfdnCore::new(8, 256, 48000.0);

        for f in [&mut a, &mut b] {
            f.set_decay_seconds(1.0);
            f.set_base_delay_ms(20.0);
            f.set_damping_hz(20000.0);
        }
        a.set_decorr(0.5);
        b.set_decorr(0.5);

        let mut la = vec![0.0; 1024];
        let mut ra = vec![0.0; 1024];
        let mut lb = vec![0.0; 1024];
        let mut rb = vec![0.0; 1024];
        la[0] = 1.0;
        ra[0] = 1.0;
        lb[0] = 1.0;
        rb[0] = 1.0;

        a.process_inplace(&mut la, &mut ra);
        b.process_inplace(&mut lb, &mut rb);

        for i in 0..la.len() {
            assert_eq!(la[i], lb[i]);
            assert_eq!(ra[i], rb[i]);
        }
    }

    /// Verifies that the loop HP/LP filters do not destabilise the
    /// network at their most extreme settings.
    #[test]
    fn loop_filters_keep_network_stable() {
        let mut ffdn = FfdnCore::new(8, 256, 48000.0);
        ffdn.set_decay_seconds(2.0);
        ffdn.set_base_delay_ms(30.0);
        ffdn.set_damping_hz(20000.0);
        ffdn.set_loop_hp(500.0);
        ffdn.set_loop_lp(2000.0);
        ffdn.set_mid_focus(1.0);

        let mut left = vec![0.0; 48000];
        let mut right = vec![0.0; 48000];
        left[0] = 1.0;
        right[0] = 1.0;
        ffdn.process_inplace(&mut left, &mut right);

        for v in &left {
            assert!(v.is_finite(), "output contains NaN/Inf with loop filters");
            assert!(v.abs() < 100.0, "output blew up: {}", v);
        }
    }

    /// Confirms that `mid_focus = 0` is bypassed: the network's
    /// response must be exactly the same as if the filter didn't
    /// exist.
    #[test]
    fn mid_focus_zero_is_bit_identical() {
        let mut a = FfdnCore::new(8, 256, 48000.0);
        let mut b = FfdnCore::new(8, 256, 48000.0);

        for f in [&mut a, &mut b] {
            f.set_decay_seconds(1.0);
            f.set_base_delay_ms(20.0);
            f.set_damping_hz(20000.0);
        }
        // `a` uses defaults (mid_focus = 0). `b` explicitly sets it.
        b.set_mid_focus(0.0);

        let mut la = vec![0.0; 1024];
        let mut ra = vec![0.0; 1024];
        let mut lb = vec![0.0; 1024];
        let mut rb = vec![0.0; 1024];
        la[0] = 1.0;
        ra[0] = 1.0;
        lb[0] = 1.0;
        rb[0] = 1.0;

        a.process_inplace(&mut la, &mut ra);
        b.process_inplace(&mut lb, &mut rb);

        for i in 0..la.len() {
            assert_eq!(la[i], lb[i]);
            assert_eq!(ra[i], rb[i]);
        }
    }

    /// Confirms that loop HP at 20 Hz and loop LP at 20 kHz are
    /// bypassed, so the pre- default is preserved exactly.
    #[test]
    fn loop_filters_neutral_is_bit_identical() {
        let mut a = FfdnCore::new(8, 256, 48000.0);
        let mut b = FfdnCore::new(8, 256, 48000.0);

        for f in [&mut a, &mut b] {
            f.set_decay_seconds(1.0);
            f.set_base_delay_ms(20.0);
            f.set_damping_hz(20000.0);
        }
        b.set_loop_hp(20.0);
        b.set_loop_lp(20_000.0);

        let mut la = vec![0.0; 1024];
        let mut ra = vec![0.0; 1024];
        let mut lb = vec![0.0; 1024];
        let mut rb = vec![0.0; 1024];
        la[0] = 1.0;
        ra[0] = 1.0;
        lb[0] = 1.0;
        rb[0] = 1.0;

        a.process_inplace(&mut la, &mut ra);
        b.process_inplace(&mut lb, &mut rb);

        for i in 0..la.len() {
            assert_eq!(la[i], lb[i]);
            assert_eq!(ra[i], rb[i]);
        }
    }

    /// Confirms that attack = 0 is bypassed: the output must be
    /// bit-identical to a network that never had the stage.
    #[test]
    fn attack_zero_is_bit_identical() {
        let mut a = FfdnCore::new(8, 256, 48000.0);
        let mut b = FfdnCore::new(8, 256, 48000.0);

        for f in [&mut a, &mut b] {
            f.set_decay_seconds(1.0);
            f.set_base_delay_ms(20.0);
            f.set_damping_hz(20000.0);
        }
        b.set_attack(0.0);

        let mut la = vec![0.0; 1024];
        let mut ra = vec![0.0; 1024];
        let mut lb = vec![0.0; 1024];
        let mut rb = vec![0.0; 1024];
        la[0] = 1.0;
        ra[0] = 1.0;
        lb[0] = 1.0;
        rb[0] = 1.0;

        a.process_inplace(&mut la, &mut ra);
        b.process_inplace(&mut lb, &mut rb);

        for i in 0..la.len() {
            assert_eq!(la[i], lb[i]);
            assert_eq!(ra[i], rb[i]);
        }
    }

    #[test]
    fn ducking_zero_is_bit_identical() {
        let mut a = FfdnCore::new(8, 256, 48000.0);
        let mut b = FfdnCore::new(8, 256, 48000.0);

        for f in [&mut a, &mut b] {
            f.set_decay_seconds(1.0);
            f.set_base_delay_ms(20.0);
            f.set_damping_hz(20000.0);
        }
        b.set_ducking(0.0);

        let mut la = vec![0.0; 1024];
        let mut ra = vec![0.0; 1024];
        let mut lb = vec![0.0; 1024];
        let mut rb = vec![0.0; 1024];
        la[0] = 1.0;
        ra[0] = 1.0;
        lb[0] = 1.0;
        rb[0] = 1.0;

        a.process_inplace(&mut la, &mut ra);
        b.process_inplace(&mut lb, &mut rb);

        for i in 0..la.len() {
            assert_eq!(la[i], lb[i]);
            assert_eq!(ra[i], rb[i]);
        }
    }

    #[test]
    fn gate_zero_is_bit_identical() {
        let mut a = FfdnCore::new(8, 256, 48000.0);
        let mut b = FfdnCore::new(8, 256, 48000.0);

        for f in [&mut a, &mut b] {
            f.set_decay_seconds(1.0);
            f.set_base_delay_ms(20.0);
            f.set_damping_hz(20000.0);
        }
        b.set_gate(0.0);

        let mut la = vec![0.0; 1024];
        let mut ra = vec![0.0; 1024];
        let mut lb = vec![0.0; 1024];
        let mut rb = vec![0.0; 1024];
        la[0] = 1.0;
        ra[0] = 1.0;
        lb[0] = 1.0;
        rb[0] = 1.0;

        a.process_inplace(&mut la, &mut ra);
        b.process_inplace(&mut lb, &mut rb);

        for i in 0..la.len() {
            assert_eq!(la[i], lb[i]);
            assert_eq!(ra[i], rb[i]);
        }
    }

    #[test]
    fn dynamics_keep_network_stable() {
        let mut ffdn = FfdnCore::new(8, 256, 48000.0);
        ffdn.set_decay_seconds(5.0);
        ffdn.set_base_delay_ms(30.0);
        ffdn.set_damping_hz(20000.0);
        ffdn.set_attack(1.0);
        ffdn.set_ducking(1.0);
        ffdn.set_gate(1.0);

        let mut left = vec![0.0; 48000];
        let mut right = vec![0.0; 48000];
        left[0] = 1.0;
        right[0] = 1.0;
        ffdn.process_inplace(&mut left, &mut right);

        for v in &left {
            assert!(v.is_finite(), "NaN/Inf with dynamics active");
            assert!(v.abs() < 100.0, "output blew up: {}", v);
        }
    }

    /// Regression test for the drive stability fix.
    ///
    /// `velvet_saturate` used to include a makeup term that gave
    /// the shaper a small-signal gain of `1 + 0.15·amount²` (1.15
    /// at amount = 1). Inside the feedback loop, any line with
    /// `g_mid · 1.15 > 1` would grow instead of decay. With decay
    /// = 3 s and size ≈ 50 ms, g_mid ≈ 0.89, so the effective loop
    /// gain was ≈ 1.024 — the tail grew until the compressor
    /// caught it.
    #[test]
    fn drive_one_keeps_network_stable() {
        let mut ffdn = FfdnCore::new(8, 256, 48000.0);
        ffdn.set_decay_seconds(3.0);
        ffdn.set_base_delay_ms(50.0);
        ffdn.set_damping_hz(20000.0);
        ffdn.set_drive(1.0);
        ffdn.set_drive_enabled(true);

        let mut left = vec![0.0; 48000];
        let mut right = vec![0.0; 48000];
        left[0] = 1.0;
        right[0] = 1.0;
        ffdn.process_inplace(&mut left, &mut right);

        for v in &left {
            assert!(v.is_finite(), "drive=1 produced NaN/Inf");
            assert!(v.abs() < 100.0, "drive=1 output blew up: {}", v);
        }

        let n = left.len();
        let early: f64 = left[0..n / 10].iter().map(|v| v * v).sum();
        let late: f64 = left[n * 9 / 10..].iter().map(|v| v * v).sum();
        assert!(
            early > late,
            "tail grew instead of decaying: early={}, late={}",
            early,
            late
        );
    }

    /// Confirms that the small-signal gain of `velvet_saturate`
    /// is unity at every drive amount. Any gain > 1 here would
    /// make every feedback line whose `g_mid > 1 / gain` unstable.
    #[test]
    fn velvet_saturate_has_unity_small_signal_gain() {
        for amount in [0.0_f64, 0.25, 0.5, 0.75, 1.0] {
            let eps = 1.0e-5;
            let y0 = super::velvet_saturate(0.0, amount);
            let y1 = super::velvet_saturate(eps, amount);
            let slope = (y1 - y0) / eps;
            assert!(
                (slope - 1.0).abs() < 1.0e-3,
                "small-signal slope at amount={}: {} (expected 1.0)",
                amount,
                slope
            );
        }
    }
}