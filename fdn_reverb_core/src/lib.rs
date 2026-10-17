//! # PEJVAK RVB – CloudSeed Legacy Core (GPL-3.0)
//!
//! Faithful Rust port of Valdemar Erlingsson's CloudReverb legacy-v1.
//! The previous "Pejvak Engine" (hybrid FDN + CLOUD morph) has been removed
//! from the tree to keep the port focused on a single, verifiable goal:
//! identical sound to the C++ original.
//!
//! On top of the CloudSeed port, this crate also hosts the PEJVAK
//! Texture Rack — a set of post-reverb texture layers (Shimmer, Crystal,
//! Mist, Scatter, Drift, Ghost) that transform the wet tail into a
//! hybrid reverb.

pub mod cloudseed;
pub mod ffdn;
pub mod texture;

pub use texture::illusion::Chime;

pub use cloudseed::params::{scaled_parameter_value, Parameter, PARAM_COUNT};
pub use cloudseed::{
    AllpassDiffuser, Biquad, ChannelLR, DelayLine, FilterType, ModulatedAllpass, ModulatedDelay,
    MultitapDiffuser, ReverbChannel, ReverbController, MAX_STAGE_COUNT, MAX_TAPS, TOTAL_LINE_COUNT,
};
pub use texture::{TextureLayer, TextureRack};
