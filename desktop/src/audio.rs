//! Turns captured audio into the 16 kHz mono that recognition expects.
//!
//! Windows hands over whatever its output device uses, so it needs this. It is plain code with no
//! operating system in it, so it can be tested anywhere.

use soundveil_core::SAMPLE_RATE;
use std::f64::consts::PI;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SampleKind {
    Float32,
    Int16,
    Int24,
    Int32,
}

impl SampleKind {
        pub fn bytes(self) -> usize {
        match self {
            Self::Float32 | Self::Int32 => 4,
            Self::Int24 => 3,
            Self::Int16 => 2,
        }
    }

    fn value(self, raw: &[u8]) -> f32 {
        match self {
            Self::Float32 => f32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]),
            Self::Int16 => i16::from_le_bytes([raw[0], raw[1]]) as f32 / 32_768.0,
            Self::Int24 => {
                i32::from_le_bytes([0, raw[0], raw[1], raw[2]]) as f32 / 2_147_483_648.0
            }
            Self::Int32 => {
                i32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]) as f32 / 2_147_483_648.0
            }
        }
    }
}

/// Reads interleaved audio into one channel by averaging all of them. Speech is usually mixed to
/// the centre, so it survives this, and it costs less than picking a channel that may be silent.
pub fn mix_down(data: &[u8], kind: SampleKind, channels: usize, out: &mut Vec<f32>) {
    let width = kind.bytes();
    for frame in data.chunks_exact(width * channels.max(1)) {
        let sum: f32 = frame.chunks_exact(width).map(|raw| kind.value(raw)).sum();
        out.push(sum / channels.max(1) as f32);
    }
}

const PHASES: usize = 64;
const ZERO_CROSSINGS: f64 = 16.0;

fn sinc(x: f64) -> f64 {
    if x.abs() < 1e-9 {
        1.0
    } else {
        (PI * x).sin() / (PI * x)
    }
}

fn blackman(place: f64) -> f64 {
    0.42 + 0.5 * (PI * place).cos() + 0.08 * (2.0 * PI * place).cos()
}

/// Converts a stream to 16 kHz with a windowed-sinc filter, which keeps speech clear and stops
/// high sounds folding down into it.
pub struct Resampler {
    step: f64,
    reach: usize,
    filter: Vec<f32>,
    input: Vec<f32>,
    position: f64,
}

impl Resampler {
    pub fn new(rate: u32) -> Self {
        let step = rate as f64 / SAMPLE_RATE as f64;
        // Slightly below half the target rate, so the filter has room to fall away.
        let cutoff = (0.92 / step).min(1.0);
        let reach = (ZERO_CROSSINGS / cutoff).ceil() as usize;
        let mut filter = Vec::with_capacity((PHASES + 1) * reach * 2);
        for phase in 0..=PHASES {
            let shift = phase as f64 / PHASES as f64;
            let row: Vec<f64> = (0..reach * 2)
                .map(|tap| {
                    let x = tap as f64 - (reach as f64 - 1.0) - shift;
                    if x.abs() >= reach as f64 {
                        0.0
                    } else {
                        sinc(cutoff * x) * blackman(x / reach as f64)
                    }
                })
                .collect();
            let total: f64 = row.iter().sum();
            filter.extend(row.iter().map(|value| (value / total) as f32));
        }
        Self {
            step,
            reach,
            filter,
            input: vec![0.0; reach],
            position: reach as f64 - 1.0,
        }
    }

    pub fn process(&mut self, samples: &[f32], out: &mut Vec<f32>) {
        if self.step == 1.0 {
            out.extend_from_slice(samples);
            return;
        }
        self.input.extend_from_slice(samples);
        let width = self.reach * 2;
        loop {
            let whole = self.position.floor() as usize;
            if whole + self.reach >= self.input.len() {
                break;
            }
            let phase = ((self.position - whole as f64) * PHASES as f64).round() as usize;
            let taps = &self.filter[phase * width..(phase + 1) * width];
            let first = whole + 1 - self.reach;
            let value = self.input[first..first + width]
                .iter()
                .zip(taps)
                .map(|(sample, tap)| sample * tap)
                .sum();
            out.push(value);
            self.position += self.step;
        }
        let done = (self.position.floor() as usize + 1)
            .saturating_sub(self.reach)
            .min(self.input.len());
        self.input.drain(..done);
        self.position -= done as f64;
    }
}

/// Some systems send nothing at all while no app is playing, but speech detection needs to hear
/// that quiet to end a sentence. This says how much silence has gone missing.
pub struct QuietClock {
    last: Instant,
}

impl QuietClock {
    const WAIT: Duration = Duration::from_millis(100);

    pub fn new(now: Instant) -> Self {
        Self { last: now }
    }

    pub fn heard(&mut self, now: Instant) {
        self.last = now;
    }

    /// Samples of silence to add, once the quiet has lasted long enough to be real.
    pub fn missing(&mut self, now: Instant) -> usize {
        let quiet = now.saturating_duration_since(self.last);
        if quiet < Self::WAIT {
            return 0;
        }
        self.last = now;
        (quiet.as_secs_f64() * SAMPLE_RATE as f64) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(hertz: f64, rate: u32, seconds: f64) -> Vec<f32> {
        let count = (rate as f64 * seconds) as usize;
        (0..count)
            .map(|n| (2.0 * PI * hertz * n as f64 / rate as f64).sin() as f32 * 0.5)
            .collect()
    }

    fn power(samples: &[f32]) -> f32 {
        // Skips the start, where the filter is still filling.
        let body = &samples[samples.len() / 4..];
        (body.iter().map(|s| s * s).sum::<f32>() / body.len() as f32).sqrt()
    }

    fn convert(samples: &[f32], rate: u32, chunk: usize) -> Vec<f32> {
        let mut resampler = Resampler::new(rate);
        let mut out = Vec::new();
        for part in samples.chunks(chunk) {
            resampler.process(part, &mut out);
        }
        out
    }

    #[test]
    fn speech_frequencies_pass_at_their_level() {
        for rate in [44_100, 48_000, 96_000] {
            let out = convert(&tone(1_000.0, rate, 1.0), rate, 480);
            let expected = 0.5 / 2_f32.sqrt();
            assert!((power(&out) - expected).abs() < 0.005, "{rate}");
        }
    }

    #[test]
    fn sounds_above_the_new_limit_do_not_fold_down() {
        for rate in [44_100, 48_000, 96_000] {
            let out = convert(&tone(12_000.0, rate, 1.0), rate, 480);
            assert!(power(&out) < 0.001, "{rate}: {}", power(&out));
        }
    }

    #[test]
    fn output_length_follows_the_rate() {
        let out = convert(&tone(440.0, 48_000, 2.0), 48_000, 1_000);
        assert!((out.len() as i64 - 32_000).abs() < 40, "{}", out.len());
        let out = convert(&tone(440.0, 44_100, 2.0), 44_100, 777);
        assert!((out.len() as i64 - 32_000).abs() < 40, "{}", out.len());
    }

    #[test]
    fn chunk_size_does_not_change_the_result() {
        let input = tone(700.0, 48_000, 0.5);
        let whole = convert(&input, 48_000, input.len());
        let parts = convert(&input, 48_000, 137);
        assert_eq!(whole.len(), parts.len());
        for (a, b) in whole.iter().zip(&parts) {
            assert!((a - b).abs() < 1e-5);
        }
    }

    #[test]
    fn audio_already_at_16k_is_left_alone() {
        let input = tone(700.0, 16_000, 0.1);
        assert_eq!(convert(&input, 16_000, 50), input);
    }

    #[test]
    fn long_runs_keep_time() {
        let mut resampler = Resampler::new(44_100);
        let mut out = Vec::new();
        for _ in 0..30 {
            out.clear();
            resampler.process(&vec![0.0; 44_100], &mut out);
            assert!((out.len() as i64 - 16_000).abs() <= 80, "{}", out.len());
            assert!(resampler.input.len() < 2_000);
        }
    }

    #[test]
    fn channels_are_averaged() {
        let bytes: Vec<u8> = [0.5_f32, -0.5, 1.0, 0.0]
            .iter()
            .flat_map(|sample| sample.to_le_bytes())
            .collect();
        let mut out = Vec::new();
        mix_down(&bytes, SampleKind::Float32, 2, &mut out);
        assert_eq!(out, [0.0, 0.5]);
    }

    #[test]
    fn integer_formats_read_as_fractions() {
        let mut out = Vec::new();
        mix_down(&i16::MIN.to_le_bytes(), SampleKind::Int16, 1, &mut out);
        mix_down(&[0, 0, 0x40], SampleKind::Int24, 1, &mut out);
        mix_down(&i32::MAX.to_le_bytes(), SampleKind::Int32, 1, &mut out);
        assert_eq!(out[0], -1.0);
        assert_eq!(out[1], 0.5);
        assert!((out[2] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn incomplete_frames_are_ignored() {
        let mut out = Vec::new();
        mix_down(&[0; 7], SampleKind::Float32, 2, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn quiet_is_counted_once_it_is_real() {
        let start = Instant::now();
        let mut clock = QuietClock::new(start);
        assert_eq!(clock.missing(start + Duration::from_millis(50)), 0);
        assert_eq!(clock.missing(start + Duration::from_millis(250)), 4_000);
        assert_eq!(clock.missing(start + Duration::from_millis(260)), 0);
    }

    #[test]
    fn hearing_audio_resets_the_quiet() {
        let start = Instant::now();
        let mut clock = QuietClock::new(start);
        clock.heard(start + Duration::from_millis(90));
        assert_eq!(clock.missing(start + Duration::from_millis(150)), 0);
    }
}
