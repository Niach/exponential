"""NEW-129: livekit-wakeword's stages cut for a shared machine.

    python staged.py <config.yaml> <stage>

The same work as `livekit-wakeword generate|augment|train`, in pieces short
enough for one heavy-lane take each, a few processes wide:

  generate-fast   positives and negatives in two processes (resumes like `generate`)
  augment-clips   `augment`'s first half, one process per split
  features        `augment`'s second half, one process per split; a finished split is kept
  train-phase     the next of the trainer's three phases, its state saved after
                  each; the third one averages the checkpoints and writes <name>.pt
"""

from __future__ import annotations

import multiprocessing
import os
import random
import sys
from pathlib import Path

WORKERS = 4
SPLITS = [
    ("positive_train", "positive_features_train.npy"),
    ("negative_train", "negative_features_train.npy"),
    ("positive_test", "positive_features_test.npy"),
    ("negative_test", "negative_features_test.npy"),
    ("background_train", "background_noise_features_train.npy"),
    ("background_test", "background_noise_features_test.npy"),
]


def _config(path: str):
    from livekit.wakeword.config import load_config

    return load_config(path)


def _quiet_threads() -> None:
    import torch

    torch.set_num_threads(2)
    seed = int.from_bytes(os.urandom(4), "little")
    random.seed(seed)
    import numpy as np

    np.random.seed(seed)
    torch.manual_seed(seed)


def _generate(job: tuple[str, str]) -> None:
    path, kind = job
    config = _config(path)
    _quiet_threads()
    from livekit.wakeword.data import generate as g
    from livekit.wakeword.data.tts import get_tts_backend

    tts = get_tts_backend(config)
    tts.validate_artifacts()
    out = config.model_output_dir
    if kind == "positive":
        phrases = list(config.target_phrases)
    else:
        phrases = g.generate_adversarial_phrases(target_phrases=config.target_phrases)
        phrases.extend(config.custom_negative_phrases)
    for split, target in ((f"{kind}_train", config.n_samples), (f"{kind}_test", config.n_samples_val)):
        existing = g._count_original_clips(out / split)
        if existing < target:
            tts.synthesize_clips(phrases=phrases, output_dir=out / split, n_samples=target, start_index=existing, batch_size=config.tts_batch_size)
    if kind == "negative":
        g._generate_background_clips(config, "background_train", config.n_background_samples)
        g._generate_background_clips(config, "background_test", config.n_background_samples_val)


def _augment(job: tuple[str, str]) -> None:
    path, split = job
    config = _config(path)
    _quiet_threads()
    import re

    from livekit.wakeword.data.augment import AudioAugmentor, _augment_directory

    clip_dir = config.model_output_dir / split
    if not clip_dir.exists():
        return
    augmented = re.compile(r"^clip_\d{6}_r\d+\.wav$")
    for stale in [p for p in clip_dir.glob("*.wav") if augmented.match(p.name)]:
        stale.unlink()
    augmentor = AudioAugmentor(
        background_paths=[Path(p) for p in config.augmentation.background_paths],
        rir_paths=[Path(p) for p in config.augmentation.rir_paths],
    )
    for round_idx in range(config.augmentation.rounds):
        _augment_directory(clip_dir, augmentor, is_positive="positive" in split, round_idx=round_idx, target_duration_s=config.augmentation.clip_duration)


def _features(job: tuple[str, str, str]) -> None:
    path, split, filename = job
    config = _config(path)
    out = config.model_output_dir / filename
    clip_dir = config.model_output_dir / split
    if out.exists() or not clip_dir.exists():
        return
    import numpy as np
    import onnxruntime as ort

    from livekit.wakeword.data.features import extract_features_from_directory
    from livekit.wakeword.models.feature_extractor import MelSpectrogramFrontend, SpeechEmbedding
    from livekit.wakeword.resources import get_embedding_model_path, get_mel_model_path

    options = ort.SessionOptions()
    options.intra_op_num_threads = 2
    options.inter_op_num_threads = 1
    features = extract_features_from_directory(
        clip_dir=clip_dir,
        mel_frontend=MelSpectrogramFrontend(onnx_path=get_mel_model_path(), sess_options=options),
        speech_embedding=SpeechEmbedding(onnx_path=get_embedding_model_path(), sess_options=options),
    )
    partial = out.with_suffix(".partial.npy")
    np.save(str(partial), features)
    partial.rename(out)
    print(f"{split}: {features.shape} -> {out}", flush=True)


def _train_phase(path: str) -> None:
    import torch

    from livekit.wakeword.training.trainer import WakeWordTrainer

    config = _config(path)
    out = config.model_output_dir
    state_path = out / "train_state.pt"
    trainer = WakeWordTrainer(config)
    state = {"done": 0, "max_neg_w": config.max_negative_weight, "checkpoints": [], "metrics_log": []}
    if state_path.exists():
        state = torch.load(state_path, map_location=trainer.device, weights_only=False)
        trainer.model.load_state_dict(state["model"])
        trainer.checkpoints = state["checkpoints"]
        trainer._metrics_log = state["metrics_log"]
    phase = state["done"] + 1
    if phase > 3:
        print("training already finished", flush=True)
        return
    import time

    trainer._train_start = time.time()
    steps = config.steps
    done_steps = {1: 0, 2: steps, 3: steps + steps // 10}[phase]
    # The same sequence as WakeWordTrainer.train(), one phase per call.
    if phase > 1:
        metrics = trainer._validate()
        trainer._log_metrics(done_steps, phase - 1, metrics)
        if metrics["fpph"] > config.target_fp_per_hour:
            state["max_neg_w"] *= 2
            print(f"false accepts still high: max_negative_weight = {state['max_neg_w']}", flush=True)
    trainer._train_phase(
        phase=phase,
        steps=steps if phase == 1 else steps // 10,
        base_lr=config.learning_rate * {1: 1.0, 2: 0.1, 3: 0.01}[phase],
        max_negative_weight=state["max_neg_w"],
        dataloader=trainer._build_dataloader(),
    )
    state.update(done=phase, model=trainer.model.state_dict(), checkpoints=trainer.checkpoints, metrics_log=trainer._metrics_log)
    partial = state_path.with_suffix(".partial")
    torch.save(state, partial)
    partial.rename(state_path)
    print(f"phase {phase} done", flush=True)
    if phase == 3:
        trainer._log_metrics(steps + 2 * (steps // 10), 3, trainer._validate())
        trainer._average_best_checkpoints()
        trainer.save(out / f"{config.model_name}.pt")


def main() -> None:
    path, stage = sys.argv[1], sys.argv[2]
    pool = multiprocessing.get_context("spawn").Pool
    if stage == "generate-fast":
        with pool(2) as workers:
            workers.map(_generate, [(path, "positive"), (path, "negative")], chunksize=1)
    elif stage == "augment-clips":
        with pool(WORKERS) as workers:
            workers.map(_augment, [(path, split) for split, _ in SPLITS], chunksize=1)
    elif stage == "features":
        with pool(WORKERS) as workers:
            workers.map(_features, [(path, split, filename) for split, filename in SPLITS], chunksize=1)
    elif stage == "train-phase":
        _train_phase(path)
    else:
        raise SystemExit(f"unknown stage: {stage}")


if __name__ == "__main__":
    main()
