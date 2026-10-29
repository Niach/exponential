//! A streaming linear resampler onto [`super::super::SAMPLE_RATE`] (the
//! idea of `crates/etalk-engine/src/capture.rs` `Resampler`; talk cannot depend on
//! voice). Speech-grade: no anti-alias filter beyond what linear gives.

pub(super) struct Resampler {
    from: f64,
    /// Input samples per output sample.
    step: f64,
    /// The next output's position, in input samples after `previous`.
    position: f64,
    previous: Option<f32>,
}

impl Resampler {
    pub(super) fn new(from: f64, to: f64) -> Self {
        Resampler { from, step: from / to, position: 0.0, previous: None }
    }

    pub(super) fn from(&self) -> f64 {
        self.from
    }

    pub(super) fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        if input.is_empty() {
            return;
        }
        if self.step == 1.0 {
            out.extend_from_slice(input);
            return;
        }
        let mut buffer = Vec::with_capacity(input.len() + 1);
        buffer.extend(self.previous);
        buffer.extend_from_slice(input);
        while self.position + 1.0 < buffer.len() as f64 {
            let index = self.position.floor() as usize;
            let fraction = (self.position - index as f64) as f32;
            out.push(buffer[index] * (1.0 - fraction) + buffer[index + 1] * fraction);
            self.position += self.step;
        }
        self.position -= (buffer.len() - 1) as f64;
        self.previous = buffer.last().copied();
    }
}

#[cfg(test)]
mod tests {
    use super::Resampler;

    #[test]
    fn three_to_one_keeps_the_count_across_chunks() {
        let mut resampler = Resampler::new(48_000.0, 16_000.0);
        let mut out = Vec::new();
        for chunk in (0..4800).collect::<Vec<_>>().chunks(441) {
            let samples: Vec<f32> = chunk.iter().map(|i| *i as f32).collect();
            resampler.process(&samples, &mut out);
        }
        assert!((out.len() as i64 - 1600).abs() <= 1, "{}", out.len());
        // A ramp stays a ramp: every third input sample.
        assert!(out.windows(2).all(|pair| (pair[1] - pair[0] - 3.0).abs() < 1e-3));
    }

    #[test]
    fn same_rate_passes_through() {
        let mut resampler = Resampler::new(16_000.0, 16_000.0);
        let mut out = Vec::new();
        resampler.process(&[0.5, -0.5], &mut out);
        assert_eq!(out, vec![0.5, -0.5]);
    }
}
