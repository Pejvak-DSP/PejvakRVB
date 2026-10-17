//! PEJVAK Texture Rack – GPL-3.0
//!
//! Six layers of post-reverb texture effects that sit between the FDN's
//! wet output and the final stereo mixer. Each layer has its own 0..1
//! `mix` parameter; at `mix = 0` the layer is skipped entirely.
//!
//! When all layers are at 0, the rack is a strict pass-through and the
//! plugin behaves bit-identically to plain CloudReverb.
//!
//! Layers are processed in series, in the order they are registered by
//! `TextureRack::new`:
//!
//!   Shimmer → Mist → Drift → Crystal → Scatter → Bloom
//!
//! Every layer is allocation-free on the audio thread; the rack itself
//! does no allocation in `process()`.

/// A single texture layer in the rack.
///
/// Implementations must be real-time safe: no allocation, no blocking
/// I/O inside `process_inplace`. Each layer owns all its state; the rack
/// only calls `set_samplerate` / `set_mix` / `process_inplace` on it.
pub trait TextureLayer: Send {
    fn process_inplace(&mut self, left: &mut [f64], right: &mut [f64]);
    fn clear_buffers(&mut self);
    fn set_samplerate(&mut self, samplerate: f64);
    fn set_mix(&mut self, mix: f64);
    fn mix(&self) -> f64;
    fn needs_processing(&self) -> bool;
    fn name(&self) -> &'static str;
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}

/// The PEJVAK Texture Rack.
pub struct TextureRack {
    layers: Vec<Box<dyn TextureLayer>>,
    /// Processing order. `order[i]` is the index into `layers` that
    /// should be processed at step `i` of the chain. Default is
    /// `[0, 1, 2, 3, 4, 5]`, i.e. the same order the layers were
    /// registered in.
    ///
    /// The user can reorder the chain at runtime via `set_order`. The
    /// audio thread reads this array once per `process` call, so a
    /// change is picked up on the very next block. Internal buffers
    /// inside each layer are untouched — only their position in the
    /// chain changes.
    order: [usize; 6],
    scratch_l: Vec<f64>,
    scratch_r: Vec<f64>,
}

pub mod shimmer;
pub use shimmer::Shimmer;

pub mod mist;
pub use mist::Mist;

pub mod drift;
pub use drift::Drift;

pub mod crystal;
pub use crystal::Crystal;

pub mod scatter;
pub use scatter::Scatter;

pub mod bloom;
pub use bloom::Bloom;

pub mod illusion;
pub use illusion::Chime;

impl TextureRack {
    /// Constructs the rack and its six layers.
    ///
    /// `buffer_size` must match `ReverbController::BUFFER_SIZE`.
    ///
    /// `samplerate` is forwarded to every layer at construction time so
    /// that all internal buffer sizes, filter coefficients, and grain
    /// lengths are correct from the very first sample.
    pub fn new(buffer_size: usize, samplerate: f64) -> Self {
        // Order matters — the rack processes layers in series, so a
        // layer's output feeds the next.
        let layers: Vec<Box<dyn TextureLayer>> = vec![
            Box::new(Shimmer::new(buffer_size, samplerate)),
            Box::new(Mist::new(buffer_size, samplerate)),
            Box::new(Drift::new(buffer_size, samplerate)),
            Box::new(Crystal::new(buffer_size, samplerate)),
            Box::new(Scatter::new(buffer_size, samplerate)),
            Box::new(Bloom::new(buffer_size, samplerate)),
        ];
        Self {
            layers,
            order: [0, 1, 2, 3, 4, 5],
            scratch_l: vec![0.0; buffer_size],
            scratch_r: vec![0.0; buffer_size],
        }
    }

    /// Returns true while any layer is either active or still fading
    /// out. Computed dynamically from each layer's `needs_processing()`,
    /// so the rack has no separate "active" flag to keep in sync.
    #[inline]
    pub fn is_active(&self) -> bool {
        self.layers.iter().any(|l| l.needs_processing())
    }

    /// No-op. Kept as a public method so existing callers continue to
    /// compile; `is_active()` is now computed dynamically.
    pub fn recompute_active(&mut self) {
        // Intentionally empty.
    }

    /// Number of layers currently registered (for the UI).
    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }

    /// Names of all registered layers, in processing order.
    pub fn layer_names(&self) -> Vec<&'static str> {
        self.layers.iter().map(|l| l.name()).collect()
    }

    /// Update the sample rate of every layer at runtime. Used when the
    /// host changes the project's sample rate without a full rebuild of
    /// the DSP engine.
    pub fn set_samplerate(&mut self, samplerate: f64) {
        for layer in &mut self.layers {
            layer.set_samplerate(samplerate);
        }
    }

    /// Replaces the processing order.
    ///
    /// `order[i]` must be the index of the layer to process at step
    /// `i`. The array must be a valid permutation of `0..6`, otherwise
    /// the call is ignored.
    pub fn set_order(&mut self, order: [usize; 6]) {
        // Validate: must be a permutation of 0..6.
        let mut seen = [false; 6];
        for &idx in &order {
            if idx >= 6 || seen[idx] {
                return;
            }
            seen[idx] = true;
        }
        self.order = order;
    }

    pub fn clear_buffers(&mut self) {
        self.scratch_l.fill(0.0);
        self.scratch_r.fill(0.0);
        for layer in &mut self.layers {
            layer.clear_buffers();
        }
    }

    /// Set the mix of a specific layer by name. No-op if the name is
    /// unknown.
    ///
    /// This is the string-keyed convenience entry point. The plugin
    /// itself uses the typed `ReverbController::set_texture_parameter`
    /// path instead; `set_layer_mix` is exposed for tooling, tests, and
    /// any consumer that prefers names over the `TextureParamId` enum.
    pub fn set_layer_mix(&mut self, name: &str, mix: f64) {
        for layer in &mut self.layers {
            if layer.name() == name {
                layer.set_mix(mix.clamp(0.0, 1.0));
            }
        }
    }

    /// Process one stereo block through the rack.
    ///
    /// * If no layer is active: `out = in` (bit-exact copy).
    /// * Otherwise: copy `in` to the scratch buffers, run every layer
    ///   in series in place, then copy the result to `out`.
    ///
    /// The implementation never allocates on the audio thread, and it
    /// is panic-safe: `self` is destructured into disjoint field
    /// borrows, so the layer Vec is never taken out of the rack.
    pub fn process(&mut self, in_l: &[f64], in_r: &[f64], out_l: &mut [f64], out_r: &mut [f64]) {
        let n = in_l
            .len()
            .min(in_r.len())
            .min(out_l.len())
            .min(out_r.len())
            .min(self.scratch_l.len());

        // Fast path: no layer active and none fading out.
        if self.layers.is_empty() || !self.layers.iter().any(|l| l.needs_processing()) {
            out_l[..n].copy_from_slice(&in_l[..n]);
            out_r[..n].copy_from_slice(&in_r[..n]);
            return;
        }

        self.scratch_l[..n].copy_from_slice(&in_l[..n]);
        self.scratch_r[..n].copy_from_slice(&in_r[..n]);

        // Destructure so the borrow checker sees that the layers Vec,
        // the order array, and the two scratch buffers are all
        // disjoint fields of `self`.
        let Self {
            layers,
            order,
            scratch_l,
            scratch_r,
        } = self;
        let scratch_l = &mut scratch_l[..n];
        let scratch_r = &mut scratch_r[..n];

        // Process the layers in the user-configured order. `order[i]`
        // is a validated permutation, so this loop visits every layer
        // exactly once.
        for &idx in order.iter() {
            // Each layer short-circuits internally when fully faded
            // out, so we can always call it. This keeps fade-out
            // audible.
            layers[idx].process_inplace(scratch_l, scratch_r);
        }

        out_l[..n].copy_from_slice(&self.scratch_l[..n]);
        out_r[..n].copy_from_slice(&self.scratch_r[..n]);
    }

    /// Returns a `&mut Shimmer` if the rack contains a Shimmer layer.
    /// Used by the controller to forward Shimmer-specific parameters
    /// (pitch, feedback, filters) that are not part of the CloudSeed
    /// 49-parameter set.
    pub fn get_shimmer_mut(&mut self) -> Option<&mut crate::texture::shimmer::Shimmer> {
        for layer in &mut self.layers {
            if layer.name() == "Shimmer" {
                return layer
                    .as_any_mut()
                    .downcast_mut::<crate::texture::shimmer::Shimmer>();
            }
        }
        None
    }

    /// Same as `get_shimmer_mut`, for the Mist layer.
    pub fn get_mist_mut(&mut self) -> Option<&mut crate::texture::mist::Mist> {
        for layer in &mut self.layers {
            if layer.name() == "Mist" {
                return layer
                    .as_any_mut()
                    .downcast_mut::<crate::texture::mist::Mist>();
            }
        }
        None
    }

    /// Same as `get_shimmer_mut`, for the Drift layer.
    pub fn get_drift_mut(&mut self) -> Option<&mut crate::texture::drift::Drift> {
        for layer in &mut self.layers {
            if layer.name() == "Drift" {
                return layer
                    .as_any_mut()
                    .downcast_mut::<crate::texture::drift::Drift>();
            }
        }
        None
    }

    /// Same as `get_shimmer_mut`, for the Crystal layer.
    pub fn get_crystal_mut(&mut self) -> Option<&mut crate::texture::crystal::Crystal> {
        for layer in &mut self.layers {
            if layer.name() == "Crystal" {
                return layer
                    .as_any_mut()
                    .downcast_mut::<crate::texture::crystal::Crystal>();
            }
        }
        None
    }

    /// Same as `get_shimmer_mut`, for the Scatter layer.
    pub fn get_scatter_mut(&mut self) -> Option<&mut crate::texture::scatter::Scatter> {
        for layer in &mut self.layers {
            if layer.name() == "Scatter" {
                return layer
                    .as_any_mut()
                    .downcast_mut::<crate::texture::scatter::Scatter>();
            }
        }
        None
    }

    /// Same as `get_shimmer_mut`, for the Bloom layer.
    pub fn get_bloom_mut(&mut self) -> Option<&mut crate::texture::bloom::Bloom> {
        for layer in &mut self.layers {
            if layer.name() == "Bloom" {
                return layer
                    .as_any_mut()
                    .downcast_mut::<crate::texture::bloom::Bloom>();
            }
        }
        None
    }
}
