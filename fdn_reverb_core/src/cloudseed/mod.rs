//! CloudSeed legacy-v1 DSP engine – GPL-3.0
//! Faithful Rust port of Valdemar Erlingsson's CloudReverb legacy-v1.
//! All arithmetic is `f64` to match the C++ `double` type.

pub mod allpass_diffuser;
pub mod biquad;
pub mod controller;
pub mod delay_line;
pub mod fast_sin;
pub mod hp1;
pub mod lp1;
pub mod modulated_allpass;
pub mod modulated_delay;
pub mod multitap_diffuser;
pub mod params;
pub mod reverb_channel;
pub mod sha256;
pub mod sha_random;
pub mod value_tables;
pub mod velvet;

pub use allpass_diffuser::{AllpassDiffuser, MAX_STAGE_COUNT};
pub use biquad::{Biquad, FilterType};
pub use controller::{ReverbController, ReverbEngine, TextureParamId, BUFFER_SIZE};
pub use delay_line::DelayLine;
pub use modulated_allpass::ModulatedAllpass;
pub use modulated_delay::ModulatedDelay;
pub use multitap_diffuser::{MultitapDiffuser, MAX_TAPS};
pub use params::{scaled_parameter_value, Parameter, DEFAULT_PARAMS, PARAM_COUNT};
pub use reverb_channel::{ChannelLR, ReverbChannel, TOTAL_LINE_COUNT};
pub use velvet::{VelvetNoiseFilter, VN_DENSITY_STEPS, VN_SEQ_LEN};
