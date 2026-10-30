//! EXP-1424: the wake word's model files on the voice device.
//!
//! A phrase with a published model (`etalk_wakeword::models`) needs three
//! pinned files in `{data_dir}/talk/models/wakeword/`: the shared front end
//! and the phrase's classifier. They are fetched in the background the
//! first time the phrase is asked for (verified like every other model,
//! nothing in the binary), and until they are ready, or while a failed
//! fetch waits for its retry, the phrase keeps the transcript matcher.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use etalk_crispasr::artifact::{self, Artifact, Pack};
use etalk_wakeword::models::{Model, Pin, DEBOUNCE, MODELS, PATIENCE};
use etalk_wakeword::{Classifier, Detector, FrontEnd, Trigger};

use crate::retry_after;

fn dir(data_dir: &Path) -> PathBuf {
    data_dir.join("talk").join(etalk_crispasr::models::MODELS_DIR).join("wakeword")
}

fn artifact(pin: Pin) -> Artifact {
    // Once per pinned file (below): the manifest code wants a static list.
    let keep: &'static [(&'static str, &'static str)] = Box::leak(Box::new([("", pin.file)]));
    Artifact { id: pin.id, url: pin.url, sha256: pin.sha256, size: pin.size, pack: Pack::File, keep }
}

/// `model`'s files as pinned downloads, the front end first.
fn artifacts(model: &Model) -> &'static [Artifact; 3] {
    static ALL: OnceLock<Vec<(&'static str, [Artifact; 3])>> = OnceLock::new();
    let all = ALL.get_or_init(|| MODELS.iter().map(|model| (model.id, model.files().map(artifact))).collect());
    &all.iter().find(|(id, _)| *id == model.id).expect("a shipped model").1
}

/// What is loaded and fetched of the models.
#[derive(Default)]
pub struct WakeModels {
    front: Option<Arc<FrontEnd>>,
    classifiers: HashMap<&'static str, Arc<Classifier>>,
    fetch: Option<Receiver<Result<(), String>>>,
    failures: u32,
    retry_at: Option<Instant>,
}

impl WakeModels {
    /// `phrase`'s detector once its files are ready; else None, starting
    /// (or waiting for) their fetch.
    pub fn detector(&mut self, data_dir: &Path, phrase: &str) -> Option<Detector> {
        let model = etalk_wakeword::models::model_for(phrase)?;
        self.poll();
        if self.fetch.is_some() {
            return None;
        }
        let dir = dir(data_dir);
        let files = artifacts(model);
        if !files.iter().all(|file| artifact::is_ready(&dir, file)) {
            if self.retry_at.is_none_or(|at| Instant::now() >= at) {
                self.start(dir, files);
            }
            return None;
        }
        match self.load(&dir, model, files) {
            Ok(detector) => Some(detector),
            Err(err) => {
                eprintln!("wake word model {} failed to load, the transcript matcher hears it: {err}", model.id);
                self.failed();
                None
            }
        }
    }

    fn load(&mut self, dir: &Path, model: &Model, files: &[Artifact; 3]) -> Result<Detector, String> {
        // Re-hashed before every load, like the speech models.
        for file in files {
            artifact::verify(dir, file).map_err(|err| err.to_string())?;
        }
        let path = |file: &Artifact| dir.join(file.keep[0].1);
        let front = match &self.front {
            Some(front) => front.clone(),
            None => {
                let front = FrontEnd::load(&path(&files[0]), &path(&files[1])).map_err(|err| err.to_string())?;
                self.front.insert(front).clone()
            }
        };
        let classifier = match self.classifiers.get(model.id) {
            Some(classifier) => classifier.clone(),
            None => {
                let classifier = Classifier::load(&path(&files[2])).map_err(|err| err.to_string())?;
                self.classifiers.entry(model.id).or_insert(classifier).clone()
            }
        };
        Ok(Detector::new(front, classifier, Trigger::new(model.threshold, PATIENCE, DEBOUNCE), model.scale))
    }

    fn start(&mut self, dir: PathBuf, files: &'static [Artifact; 3]) {
        let (done, result) = mpsc::channel();
        let spawned = std::thread::Builder::new().name("voice-wake-download".into()).spawn(move || {
            let _ = done.send(artifact::ensure_all(&dir, files, |_, _| {}).map_err(|err| err.to_string()));
        });
        match spawned {
            Ok(_) => self.fetch = Some(result),
            Err(err) => {
                eprintln!("wake word model: no download thread: {err}");
                self.failed();
            }
        }
    }

    fn poll(&mut self) {
        let Some(fetch) = &self.fetch else { return };
        let result = match fetch.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err("the download stopped".into()),
        };
        self.fetch = None;
        match result {
            Ok(()) => self.failures = 0,
            Err(err) => {
                eprintln!("wake word model unavailable, the transcript matcher hears it: {err}");
                self.failed();
            }
        }
    }

    fn failed(&mut self) {
        self.failures += 1;
        self.retry_at = Some(Instant::now() + retry_after(self.failures));
    }
}
