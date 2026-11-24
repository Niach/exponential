"""NEW-129: livekit-wakeword's stages cut for a shared machine.

    python staged.py <config.yaml> <stage>

The same work as `livekit-wakeword generate|augment|train`, in pieces short
enough for one heavy-lane take each, a few processes wide:

  generate-fast   `generate` in 2,500-clip pieces, STAGED_WORKERS at a time; a rerun
                  goes on after the clips that exist
  augment-clips   `augment`'s first half, one process per split; a finished split is kept
  features        `augment`'s second half, one process per split; a finished split is kept
  train-resume    `train`, its state saved every 5,000 steps and on SIGTERM: a later
                  call goes on from there; the last one writes <name>.pt
                  (STAGED_VRAM_GB, STAGED_GPU_DUTY: a cap for a shared GPU)

STAGED_WORKERS (default 4) and STAGED_THREADS (default 2) bound the CPU use.
"""

from __future__ import annotations

import multiprocessing
import os
import random
import sys
from pathlib import Path

WORKERS = int(os.environ.get("STAGED_WORKERS", "4"))
THREADS = int(os.environ.get("STAGED_THREADS", "2"))
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

    torch.set_num_threads(THREADS)
    seed = int.from_bytes(os.urandom(4), "little")
    random.seed(seed)
    import numpy as np

    np.random.seed(seed)
    torch.manual_seed(seed)


def _generate(job: tuple[str, str, int, int]) -> None:
    """Clips [first, last) of one split; a rerun goes on after the clips that exist."""
    path, split, first, last = job
    config = _config(path)
    from livekit.wakeword.data import generate as g
    from livekit.wakeword.data.tts import get_tts_backend

    out = config.model_output_dir / split
    if split.startswith("background"):
        g._generate_background_clips(config, split, last)
        return
    if split.startswith("positive"):
        phrases = list(config.target_phrases)
    else:
        random.seed(129)  # every worker and every rerun cycles the same list
        phrases = g.generate_adversarial_phrases(target_phrases=config.target_phrases)
        phrases.extend(config.custom_negative_phrases)
    _quiet_threads()
    start = first
    while start < last and (out / f"clip_{start:06d}.wav").exists():
        start += 1
    if start < last:
        tts = get_tts_backend(config)
        tts.validate_artifacts()
        tts.synthesize_clips(phrases=phrases, output_dir=out, n_samples=last, start_index=start, batch_size=config.tts_batch_size)


def _generate_jobs(path: str) -> list[tuple[str, str, int, int]]:
    config = _config(path)
    jobs = []
    for split, total in (("positive_train", config.n_samples), ("negative_train", config.n_samples), ("positive_test", config.n_samples_val), ("negative_test", config.n_samples_val)):
        size = 2500
        jobs += [(path, split, first, min(first + size, total)) for first in range(0, total, size)]
    jobs += [(path, "background_train", 0, config.n_background_samples), (path, "background_test", 0, config.n_background_samples_val)]
    return jobs


def _augment(job: tuple[str, str]) -> None:
    path, split = job
    config = _config(path)
    _quiet_threads()
    import re

    from livekit.wakeword.data.augment import AudioAugmentor, _augment_directory

    clip_dir = config.model_output_dir / split
    finished = config.model_output_dir / f".augmented-{split}"
    if finished.exists() or not clip_dir.exists():
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
    finished.touch()


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
    options.intra_op_num_threads = THREADS
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


CHECKPOINT_STEPS = 5000


def _batches(trainer, start_step: int):
    """livekit-wakeword's mmap_batch_generator, starting where `start_step` batches would have left it."""
    import numpy as np

    config = trainer.config
    out = config.model_output_dir
    files = {
        "positive": out / "positive_features_train.npy",
        "adversarial_negative": out / "negative_features_train.npy",
        "ACAV100M_sample": config.data_path / "features" / "openwakeword_features_ACAV100M_2000_hrs_16bit.npy",
        "background_noise": out / "background_noise_features_train.npy",
    }
    data = {}
    for name, path in files.items():
        array = np.load(str(path), mmap_mode="r")
        if array.ndim == 2 and array.shape[1] == 96:
            array = array[: (array.shape[0] // 16) * 16].reshape(-1, 16, 96)
        assert array.ndim == 3 and array.shape[1:] == (16, 96), f"{path}: {array.shape}"
        data[name] = array
    per_class = {name: config.batch_n_per_class.get(name, 0) for name in data}
    step = start_step
    while True:
        features, labels = [], []
        for name, array in data.items():
            n = per_class[name]
            if n == 0:
                continue
            indices = np.sort(np.arange(step * n, step * n + n) % array.shape[0])
            features.append(np.asarray(array[indices]))
            labels.append(np.full(n, 1.0 if name == "positive" else 0.0, dtype=np.float32))
        features, labels = np.concatenate(features), np.concatenate(labels)
        order = np.random.permutation(len(labels))
        yield features[order].astype(np.float32), labels[order]
        step += 1


def _train(path: str) -> None:
    """WakeWordTrainer.train(), resumable: its state is saved every CHECKPOINT_STEPS and on SIGTERM/SIGINT.

    STAGED_VRAM_GB caps this process's CUDA memory, STAGED_GPU_DUTY (0…1) is the
    share of the time it computes (the rest it sleeps, so the card stays free).
    Exit 0 = finished (<name>.pt written), 3 = stopped on a checkpoint.
    """
    import signal
    import time

    import torch

    from livekit.wakeword.training import trainer as t

    config = _config(path)
    out = config.model_output_dir
    state_path = out / "train_state.pt"
    trainer = t.WakeWordTrainer(config)
    device = trainer.device
    if device.type == "cuda" and os.environ.get("STAGED_VRAM_GB"):
        total = torch.cuda.get_device_properties(0).total_memory
        torch.cuda.set_per_process_memory_fraction(float(os.environ["STAGED_VRAM_GB"]) * 2**30 / total)
    duty = float(os.environ.get("STAGED_GPU_DUTY", "1"))
    steps = config.steps
    phases = {1: steps, 2: steps // 10, 3: steps // 10}
    state = {"phase": 1, "step": 0, "max_neg_w": config.max_negative_weight, "checkpoints": [], "metrics_log": [], "optimizer": None, "elapsed": 0.0}
    if state_path.exists():
        state = torch.load(state_path, map_location=device, weights_only=False)
        trainer.model.load_state_dict(state["model"])
        trainer.checkpoints = state["checkpoints"]
        trainer._metrics_log = state["metrics_log"]
        print(f"resuming at phase {state['phase']} step {state['step']}", flush=True)
    trainer._train_start = time.time() - state["elapsed"]

    stop = {"now": False}
    for signum in (signal.SIGTERM, signal.SIGINT):
        signal.signal(signum, lambda *_: stop.update(now=True))

    def save() -> None:
        state.update(model=trainer.model.state_dict(), checkpoints=trainer.checkpoints, metrics_log=trainer._metrics_log, elapsed=time.time() - trainer._train_start)
        partial = state_path.with_suffix(".partial")
        torch.save(state, partial)
        partial.rename(state_path)

    while state["phase"] <= 3:
        phase, total = state["phase"], phases[state["phase"]]
        before = sum(phases[p] for p in range(1, phase))
        if phase > 1 and state["step"] == 0:
            # Between phases: validate, and double the negative weight while false accepts are high.
            metrics = trainer._validate()
            trainer._log_metrics(before, phase - 1, metrics)
            if metrics["fpph"] > config.target_fp_per_hour:
                state["max_neg_w"] *= 2
                print(f"false accepts still high: max_negative_weight = {state['max_neg_w']}", flush=True)
        base_lr = config.learning_rate * {1: 1.0, 2: 0.1, 3: 0.01}[phase]
        optimizer = torch.optim.AdamW(trainer.model.parameters(), lr=base_lr, weight_decay=config.weight_decay)
        if state["optimizer"] is not None:
            optimizer.load_state_dict(state["optimizer"])
        warmup, hold = (total // 5, total // 3) if phase == 1 else (0, 0)
        validation_interval = max(1, total // 20)
        batches = _batches(trainer, before + state["step"])
        trainer.model.train()
        print(f"=== phase {phase}: steps {state['step']}..{total}, lr {base_lr}, max_neg_w {state['max_neg_w']} ===", flush=True)
        started = time.time()
        for step in range(state["step"], total):
            began = time.time()
            features, labels = next(batches)
            features, labels = torch.from_numpy(features).to(device), torch.from_numpy(labels).to(device)
            lam = torch.distributions.Beta(0.2, 0.2).sample().to(device)
            perm = torch.randperm(features.size(0), device=device)
            features = lam * features + (1 - lam) * features[perm]
            labels = lam * labels + (1 - lam) * labels[perm]
            if config.label_smoothing > 0:
                labels = labels * (1 - config.label_smoothing) + 0.5 * config.label_smoothing
            predictions = trainer.model(features).squeeze(-1)
            neg_weight = t._negative_weight_schedule(step, total, state["max_neg_w"])
            loss = (t.focal_loss(predictions, labels, gamma=2.0) * torch.where(labels < 0.5, neg_weight, 1.0)).mean()
            lr = t._cosine_warmup_schedule(step, total, warmup, hold, base_lr)
            for group in optimizer.param_groups:
                group["lr"] = lr
            optimizer.zero_grad()
            loss.backward()
            optimizer.step()
            if step >= total * 3 // 4 and step % validation_interval == 0:
                metrics = trainer._validate()
                print(f"  validation @ phase {phase} step {step}: FPPH={metrics['fpph']:.2f} recall={metrics['recall']:.3f}", flush=True)
                trainer._log_metrics(before + step, phase, metrics)
                trainer._save_checkpoint(step, phase, metrics)
                trainer.model.train()
            done = step + 1
            if done % 1000 == 0:
                print(f"phase {phase} step {done}/{total} loss {loss.item():.4f} {1000 / (time.time() - started):.0f} step/s", flush=True)
                started = time.time()
            if done % CHECKPOINT_STEPS == 0 or stop["now"]:
                state.update(step=done, optimizer=optimizer.state_dict())
                save()
                if stop["now"]:
                    print(f"stopped on a checkpoint: phase {phase} step {done}", flush=True)
                    raise SystemExit(3)
            if duty < 1:
                if device.type == "cuda":
                    torch.cuda.synchronize()  # the sleep must follow the GPU's work, not its queueing
                time.sleep((time.time() - began) * (1 / duty - 1))
        state.update(phase=phase + 1, step=0, optimizer=None)
        save()
    trainer._log_metrics(sum(phases.values()), 3, trainer._validate())
    trainer._average_best_checkpoints()
    trainer.save(out / f"{config.model_name}.pt")


def main() -> None:
    path, stage = sys.argv[1], sys.argv[2]
    pool = multiprocessing.get_context("spawn").Pool
    if stage == "generate-fast":
        with pool(WORKERS) as workers:
            workers.map(_generate, _generate_jobs(path), chunksize=1)
    elif stage == "augment-clips":
        with pool(WORKERS) as workers:
            workers.map(_augment, [(path, split) for split, _ in SPLITS], chunksize=1)
    elif stage == "features":
        with pool(WORKERS) as workers:
            workers.map(_features, [(path, split, filename) for split, filename in SPLITS], chunksize=1)
    elif stage == "train-resume":
        _train(path)
    else:
        raise SystemExit(f"unknown stage: {stage}")


if __name__ == "__main__":
    main()
