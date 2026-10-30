//! EXP-1424: eTalk's wake word with a model of its own.
//!
//! An openWakeWord-compatible detector: 16 kHz mono PCM in 80 ms frames
//! ([`FRAME_SAMPLES`]) through three ONNX models, run by tract (pure Rust):
//!
//! 1. `melspectrogram.onnx`: the frame plus the [`MEL_CONTEXT`] samples
//!    before it → [`MEL_FRAMES_PER_FRAME`] frames of [`MEL_BINS`] mel bins
//!    (`x / 10 + 2`, openWakeWord's transform);
//! 2. `embedding_model.onnx` (Google's speech embedding): the last
//!    [`EMBEDDING_WINDOW`] mel frames → one [`EMBEDDING_DIM`] embedding per
//!    frame;
//! 3. a classifier per phrase (livekit-wakeword's export): the last
//!    [`CLASSIFIER_WINDOW`] embeddings (1.28 s) → a score in 0…1.
//!
//! A [`Trigger`] turns the scores into detections: [`Trigger::threshold`],
//! held for `patience` frames, then quiet for `debounce` frames.
//! The front end ([`FrontEnd`]) and the classifiers ([`Classifier`]) load
//! once and are shared; each listening stream gets its own [`Detector`].
//!
//! Which phrases have a model, and where each file downloads from (pinned by
//! sha256): [`models`]. Training: `train/` (livekit-wakeword, Piper TTS).
//! Fixture-locked in Exponential by
//! `packages/etalk/fixtures/talk-listen.json` (`wake.detector`).

use std::collections::VecDeque;
use std::path::Path;
use std::sync::Arc;

use tract_onnx::prelude::*;

pub mod models;

/// The sample rate every model expects.
pub const RATE: usize = 16_000;
/// One detector step: 80 ms.
pub const FRAME_SAMPLES: usize = 1_280;
/// Samples before a frame the mel model also reads (its FFT window reaches
/// back): 3 hops.
pub const MEL_CONTEXT: usize = 480;
pub const MEL_BINS: usize = 32;
/// Mel frames one step adds (`FRAME_SAMPLES` / the 160-sample hop).
pub const MEL_FRAMES_PER_FRAME: usize = 8;
/// Mel frames per embedding.
pub const EMBEDDING_WINDOW: usize = 76;
pub const EMBEDDING_DIM: usize = 96;
/// Embeddings per classifier input (16 × 80 ms = 1.28 s).
pub const CLASSIFIER_WINDOW: usize = 16;

/// What went wrong loading or running a model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

fn error(context: &str) -> impl Fn(TractError) -> Error + '_ {
    move |err| Error(format!("{context}: {err:#}"))
}

type Plan = Arc<TypedRunnableModel>;

/// Load `bytes` as ONNX with its one input fixed to `shape` (a symbolic
/// `batch` the export left, as livekit-wakeword's classifiers do, = 1),
/// optimized.
fn plan(bytes: &[u8], shape: &[usize], what: &str) -> Result<Plan, Error> {
    let load = || -> TractResult<Plan> {
        let mut model = tract_onnx::onnx().model_for_read(&mut std::io::Cursor::new(bytes))?;
        let batch = model.symbols.sym("batch");
        let mut dims: TVec<TDim> = shape.iter().map(|dim| TDim::from(*dim)).collect();
        dims[0] = batch.clone().into();
        model = model.with_input_fact(0, f32::fact(dims).into())?;
        let one = std::collections::HashMap::from([(batch, TDim::from(1))]);
        model.into_typed()?.set_symbols(&one)?.into_optimized()?.into_runnable()
    };
    load().map_err(error(what))
}

fn run(plan: &Plan, shape: &[usize], data: &[f32], what: &str) -> Result<Vec<f32>, Error> {
    let input = Tensor::from_shape(shape, data).map_err(error(what))?;
    let outputs = plan.run(tvec!(input.into_tvalue())).map_err(error(what))?;
    let view = outputs[0].try_as_plain_ram().map_err(error(what))?;
    Ok(view.as_slice::<f32>().map_err(error(what))?.to_vec())
}

fn read(path: &Path) -> Result<Vec<u8>, Error> {
    std::fs::read(path).map_err(|err| Error(format!("{}: {err}", path.display())))
}

/// The shared front end: the mel spectrogram and the speech embedding.
pub struct FrontEnd {
    mel: Plan,
    embedding: Plan,
}

impl FrontEnd {
    /// From the two ONNX files' bytes.
    pub fn from_bytes(mel: &[u8], embedding: &[u8]) -> Result<Arc<Self>, Error> {
        Ok(Arc::new(FrontEnd {
            mel: plan(mel, &[1, FRAME_SAMPLES + MEL_CONTEXT], "melspectrogram")?,
            embedding: plan(embedding, &[1, EMBEDDING_WINDOW, MEL_BINS, 1], "embedding")?,
        }))
    }

    pub fn load(mel: &Path, embedding: &Path) -> Result<Arc<Self>, Error> {
        Self::from_bytes(&read(mel)?, &read(embedding)?)
    }

    /// One frame's mel frames (`samples` = the context + the frame).
    fn mel(&self, samples: &[f32]) -> Result<Vec<f32>, Error> {
        let mut out = run(&self.mel, &[1, samples.len()], samples, "melspectrogram")?;
        for value in &mut out {
            *value = *value / 10.0 + 2.0;
        }
        Ok(out)
    }

    fn embed(&self, mel: &[f32]) -> Result<Vec<f32>, Error> {
        run(&self.embedding, &[1, EMBEDDING_WINDOW, MEL_BINS, 1], mel, "embedding")
    }
}

/// One phrase's classifier.
pub struct Classifier {
    plan: Plan,
}

impl Classifier {
    pub fn from_bytes(bytes: &[u8]) -> Result<Arc<Self>, Error> {
        Ok(Arc::new(Classifier { plan: plan(bytes, &[1, CLASSIFIER_WINDOW, EMBEDDING_DIM], "classifier")? }))
    }

    pub fn load(path: &Path) -> Result<Arc<Self>, Error> {
        Self::from_bytes(&read(path)?)
    }

    fn score(&self, embeddings: &[f32]) -> Result<f32, Error> {
        let out = run(&self.plan, &[1, CLASSIFIER_WINDOW, EMBEDDING_DIM], embeddings, "classifier")?;
        out.first().copied().ok_or_else(|| Error("classifier: no score".into()))
    }
}

/// Scores into detections (talk-listen.json `wake.detector.trigger`): a
/// frame scoring at least `threshold` counts; `patience` such frames in a
/// row fire once, and nothing fires again for `debounce` frames after.
#[derive(Debug, Clone, PartialEq)]
pub struct Trigger {
    pub threshold: f32,
    pub patience: usize,
    pub debounce: usize,
    above: usize,
    frame: usize,
    quiet_until: usize,
}

impl Trigger {
    pub fn new(threshold: f32, patience: usize, debounce: usize) -> Self {
        Trigger { threshold, patience: patience.max(1), debounce, above: 0, frame: 0, quiet_until: 0 }
    }

    /// The next frame's score; true = a detection at this frame.
    pub fn push(&mut self, score: f32) -> bool {
        let frame = self.frame;
        self.frame += 1;
        self.above = if score >= self.threshold { self.above + 1 } else { 0 };
        if self.above < self.patience || frame < self.quiet_until {
            return false;
        }
        self.above = 0;
        self.quiet_until = frame + 1 + self.debounce;
        true
    }
}

/// One scored frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Score {
    /// Samples pushed up to this frame's end.
    pub end: usize,
    pub score: f32,
    /// The [`Trigger`] fired here.
    pub fired: bool,
}

/// One listening stream: its frames, mel frames and embeddings so far.
pub struct Detector {
    front: Arc<FrontEnd>,
    classifier: Arc<Classifier>,
    trigger: Trigger,
    /// Multiplies every sample before the mel model (1.0 for
    /// livekit-wakeword's classifiers, trained on -1…1 PCM; 32768.0 for
    /// openWakeWord's own, trained on int16-range floats).
    scale: f32,
    /// The context and the samples not yet a whole frame.
    pending: Vec<f32>,
    mel: VecDeque<f32>,
    embeddings: VecDeque<f32>,
    consumed: usize,
}

impl Detector {
    pub fn new(front: Arc<FrontEnd>, classifier: Arc<Classifier>, trigger: Trigger, scale: f32) -> Self {
        Detector {
            front,
            classifier,
            trigger,
            scale,
            pending: vec![0.0; MEL_CONTEXT],
            // openWakeWord starts the mel buffer at ones.
            mel: std::iter::repeat_n(1.0, EMBEDDING_WINDOW * MEL_BINS).collect(),
            embeddings: VecDeque::new(),
            consumed: 0,
        }
    }

    /// Samples pushed so far (whole frames scored or not).
    pub fn samples(&self) -> usize {
        self.consumed + self.pending.len() - MEL_CONTEXT
    }

    /// Push 16 kHz mono samples (-1…1); every frame they complete is scored.
    /// The first [`CLASSIFIER_WINDOW`] - 1 frames score 0 (no history yet).
    pub fn push(&mut self, samples: &[f32]) -> Result<Vec<Score>, Error> {
        self.pending.extend(samples.iter().map(|sample| sample * self.scale));
        let mut scores = Vec::new();
        while self.pending.len() >= MEL_CONTEXT + FRAME_SAMPLES {
            let mel = self.front.mel(&self.pending[..MEL_CONTEXT + FRAME_SAMPLES])?;
            self.pending.drain(..FRAME_SAMPLES);
            self.consumed += FRAME_SAMPLES;
            self.mel.extend(mel);
            let excess = self.mel.len().saturating_sub(EMBEDDING_WINDOW * MEL_BINS);
            self.mel.drain(..excess);
            let embedding = self.front.embed(self.mel.make_contiguous())?;
            self.embeddings.extend(embedding);
            let excess = self.embeddings.len().saturating_sub(CLASSIFIER_WINDOW * EMBEDDING_DIM);
            self.embeddings.drain(..excess);
            let score = if self.embeddings.len() == CLASSIFIER_WINDOW * EMBEDDING_DIM {
                self.classifier.score(self.embeddings.make_contiguous())?
            } else {
                0.0
            };
            let fired = self.trigger.push(score);
            scores.push(Score { end: self.consumed, score, fired });
        }
        Ok(scores)
    }

    /// [`Self::push`], keeping only the first detection: where it ended
    /// (samples since the stream began).
    pub fn detect(&mut self, samples: &[f32]) -> Result<Option<usize>, Error> {
        Ok(self.push(samples)?.into_iter().find(|score| score.fired).map(|score| score.end))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fires(trigger: &mut Trigger, scores: &[f32]) -> Vec<usize> {
        scores.iter().enumerate().filter_map(|(frame, score)| trigger.push(*score).then_some(frame)).collect()
    }

    #[test]
    fn patience_needs_frames_in_a_row() {
        let mut trigger = Trigger::new(0.5, 2, 3);
        assert_eq!(fires(&mut trigger, &[0.9, 0.1, 0.9, 0.5, 0.9]), [3]);
    }

    #[test]
    fn the_debounce_holds_the_frames_after_a_detection() {
        let mut trigger = Trigger::new(0.5, 1, 3);
        assert_eq!(fires(&mut trigger, &[0.9, 0.9, 0.9, 0.9, 0.9, 0.1, 0.9, 0.1, 0.9]), [0, 4, 8]);
    }
}
