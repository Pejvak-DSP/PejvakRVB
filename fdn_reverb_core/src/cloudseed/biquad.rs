//! Biquad filter – GPL-3.0
//!
//! Faithful Rust port of `CloudSeed.Native/AudioLib/Biquad.h` (legacy-v1).
//! Uses **Direct Form I** exactly as the C++ original, with `f64` for
//! state and coefficients. The previous Rust port switched to Transposed
//! Direct Form II for numerical stability, which is a valid engineering
//! choice but produces a slightly different response in the lowest
//! octaves — audible on a long reverb tail.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterType {
    LowPass,
    HighPass,
    BandPass,
    Notch,
    Peak,
    LowShelf,
    HighShelf,
}

#[derive(Clone, Debug)]
pub struct Biquad {
    sample_rate: f64,
    q: f64,
    gain: f64,
    frequency: f64,
    slope: f64,
    filter_type: FilterType,
    a0: f64,
    a1: f64,
    a2: f64,
    b0: f64,
    b1: f64,
    b2: f64,
    // Direct Form I state (matches C++): two input samples + two output samples
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
}

impl Biquad {
    pub fn new(filter_type: FilterType, sample_rate: f64) -> Self {
        let mut b = Self {
            sample_rate,
            q: 0.5,
            gain: 1.0,
            frequency: sample_rate / 4.0,
            slope: 1.0,
            filter_type,
            a0: 1.0,
            a1: 0.0,
            a2: 0.0,
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        };
        b.update();
        b.clear_buffers();
        b
    }

    pub fn set_samplerate(&mut self, sr: f64) {
        self.sample_rate = sr;
        self.update();
    }

    pub fn set_gain_db(&mut self, db: f64) {
        self.set_gain(10.0_f64.powf(db / 20.0));
    }

    pub fn set_gain(&mut self, mut gain: f64) {
        if gain == 0.0 {
            gain = 0.001;
        }
        self.gain = gain;
        self.update();
    }

    pub fn set_q(&mut self, mut q: f64) {
        if q == 0.0 {
            q = 1e-12;
        }
        self.q = q;
        self.update();
    }

    pub fn set_frequency(&mut self, freq: f64) {
        self.frequency = freq;
        self.update();
    }

    pub fn set_slope(&mut self, slope: f64) {
        self.slope = slope;
        self.update();
    }

    pub fn set_type(&mut self, t: FilterType) {
        self.filter_type = t;
        self.update();
    }

    pub fn clear_buffers(&mut self) {
        self.x1 = 0.0;
        self.x2 = 0.0;
        self.y1 = 0.0;
        self.y2 = 0.0;
    }

    pub fn update(&mut self) {
        let omega = 2.0 * std::f64::consts::PI * self.frequency / self.sample_rate;
        let sin_omega = omega.sin();
        let cos_omega = omega.cos();

        let (alpha, sqrt_gain) = match self.filter_type {
            FilterType::LowShelf | FilterType::HighShelf => {
                let alpha = sin_omega / 2.0
                    * ((self.gain + 1.0 / self.gain) * (1.0 / self.slope - 1.0) + 2.0).sqrt();
                (alpha, self.gain.sqrt())
            }
            _ => (sin_omega / (2.0 * self.q), 1.0),
        };

        let (b0, b1, b2, a0, a1, a2) = match self.filter_type {
            FilterType::LowPass => (
                (1.0 - cos_omega) / 2.0,
                1.0 - cos_omega,
                (1.0 - cos_omega) / 2.0,
                1.0 + alpha,
                -2.0 * cos_omega,
                1.0 - alpha,
            ),
            FilterType::HighPass => (
                (1.0 + cos_omega) / 2.0,
                -(1.0 + cos_omega),
                (1.0 + cos_omega) / 2.0,
                1.0 + alpha,
                -2.0 * cos_omega,
                1.0 - alpha,
            ),
            FilterType::BandPass => (
                alpha,
                0.0,
                -alpha,
                1.0 + alpha,
                -2.0 * cos_omega,
                1.0 - alpha,
            ),
            FilterType::Notch => (
                1.0,
                -2.0 * cos_omega,
                1.0,
                1.0 + alpha,
                -2.0 * cos_omega,
                1.0 - alpha,
            ),
            FilterType::Peak => (
                1.0 + alpha * self.gain,
                -2.0 * cos_omega,
                1.0 - alpha * self.gain,
                1.0 + alpha / self.gain,
                -2.0 * cos_omega,
                1.0 - alpha / self.gain,
            ),
            FilterType::LowShelf => {
                let g = self.gain;
                (
                    g * ((g + 1.0) - (g - 1.0) * cos_omega + 2.0 * sqrt_gain * alpha),
                    2.0 * g * ((g - 1.0) - (g + 1.0) * cos_omega),
                    g * ((g + 1.0) - (g - 1.0) * cos_omega - 2.0 * sqrt_gain * alpha),
                    (g + 1.0) + (g - 1.0) * cos_omega + 2.0 * sqrt_gain * alpha,
                    -2.0 * ((g - 1.0) + (g + 1.0) * cos_omega),
                    (g + 1.0) + (g - 1.0) * cos_omega - 2.0 * sqrt_gain * alpha,
                )
            }
            FilterType::HighShelf => {
                let g = self.gain;
                (
                    g * ((g + 1.0) + (g - 1.0) * cos_omega + 2.0 * sqrt_gain * alpha),
                    -2.0 * g * ((g - 1.0) + (g + 1.0) * cos_omega),
                    g * ((g + 1.0) + (g - 1.0) * cos_omega - 2.0 * sqrt_gain * alpha),
                    (g + 1.0) - (g - 1.0) * cos_omega + 2.0 * sqrt_gain * alpha,
                    2.0 * ((g - 1.0) - (g + 1.0) * cos_omega),
                    (g + 1.0) - (g - 1.0) * cos_omega - 2.0 * sqrt_gain * alpha,
                )
            }
        };

        let inv_a0 = 1.0 / a0;
        self.a0 = a0;
        self.a1 = a1 * inv_a0;
        self.a2 = a2 * inv_a0;
        self.b0 = b0 * inv_a0;
        self.b1 = b1 * inv_a0;
        self.b2 = b2 * inv_a0;
    }

    /// Direct Form I, exactly as the C++ original.
    #[inline(always)]
    pub fn process(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2;
        self.x2 = self.x1;
        self.y2 = self.y1;
        self.x1 = x;
        self.y1 = y;
        y
    }

    /// In-place block processing.
    pub fn process_inplace(&mut self, buffer: &mut [f64]) {
        for x in buffer.iter_mut() {
            *x = self.process(*x);
        }
    }
}

impl Default for Biquad {
    fn default() -> Self {
        Self::new(FilterType::LowPass, 48_000.0)
    }
}
