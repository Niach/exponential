#!/usr/bin/env python3
"""#NEW-56: the wake word's corpus set for `cargo run -p voice --example wake`.

Mixes licensed public speech with CC0 game audio into 30-minute 16 kHz mono
WAVs (negatives), and has Qwen3-TTS CustomVoice's built-in speakers say the
wake phrases (positives). NOTHING it writes is committed: the audio stays in
the output directory (outside the repository or under the gitignored /out/);
only this script, the manifest it prints and the harness's numbers are.

    uv run --with numpy --with soundfile --with pyarrow python -I \\
        apps/desktop/spikes/talk/scripts/wake_corpus.py mix \\
        --speech <dir-or-parquet>... --background <dir>... --out <dir> \\
        [--hours 8.5] [--seed 56] [--label en]

    uv run --with numpy --with soundfile python -I \\
        apps/desktop/spikes/talk/scripts/wake_corpus.py positives \\
        --crispasr <bin> --model <customvoice.gguf> --codec <tokenizer.gguf> \\
        --out <dir>/positives --language en|de --phrase "Hey Triage"...

Speech: any FLAC/WAV under a directory (LibriSpeech, CC BY 4.0) or the
`audio` column of a parquet file (Multilingual LibriSpeech on Hugging Face,
CC BY 4.0). Background: any OGG/MP3/WAV/FLAC under a directory (Kenney and
OpenGameArt packs released under CC0), decoded with ffmpeg.

The mix, per 30-minute file, scene by scene (seeded, so it rebuilds the same):
- 80 %: a speech scene of 30-180 s: utterances in random order at
  -26 dBFS RMS +/- 6 dB, 0.2-2.5 s apart (one in eight gaps 4-8 s), over a bed
  of a quiet room (25 %), game music at 5-20 dB below the speech (50 %), or
  music plus game sound effects (25 %);
- 20 %: a game-only scene of 30-120 s: music plus effects, no speech.
A -65 dBFS noise floor runs under everything (a real microphone is never
digitally silent).
"""

import argparse
import hashlib
import io
import json
import random
import subprocess
import sys
from pathlib import Path

import numpy as np
import soundfile

RATE = 16_000
FILE_SECONDS = 30 * 60
AUDIO = {".flac", ".wav", ".ogg", ".mp3"}


def resample(audio, rate):
    if rate == RATE:
        return audio.astype(np.float32)
    length = int(round(len(audio) * RATE / rate))
    return np.interp(np.linspace(0, len(audio) - 1, length), np.arange(len(audio)), audio).astype(np.float32)


def mono(audio):
    return audio.mean(axis=1) if audio.ndim == 2 else audio


def decode_ffmpeg(path):
    raw = subprocess.run(
        ["ffmpeg", "-loglevel", "error", "-i", str(path), "-ac", "1", "-ar", str(RATE), "-f", "f32le", "-"],
        check=True,
        capture_output=True,
    ).stdout
    return np.frombuffer(raw, dtype=np.float32).copy()


def rms(audio):
    return float(np.sqrt(np.mean(np.square(audio), dtype=np.float64))) if len(audio) else 0.0


def gain_to(audio, dbfs):
    level = rms(audio)
    return audio * (10 ** (dbfs / 20) / level) if level > 1e-6 else audio


class Speech:
    """Utterances, lazily decoded: (source, id, loader)."""

    def __init__(self, inputs):
        self.items = []
        for spec in inputs:
            path = Path(spec)
            if path.suffix == ".parquet":
                import pyarrow.parquet as pq

                table = pq.read_table(path, columns=["audio", "id"])
                audio, ids = table.column("audio").to_pylist(), table.column("id").to_pylist()
                for index, (cell, ident) in enumerate(zip(audio, ids)):
                    self.items.append((path.name, str(ident), lambda cell=cell: self._bytes(cell["bytes"])))
            else:
                for file in sorted(path.rglob("*")):
                    if file.suffix.lower() in {".flac", ".wav"}:
                        self.items.append((path.name, file.stem, lambda file=file: self._file(file)))
        if not self.items:
            sys.exit("no speech found")

    @staticmethod
    def _file(file):
        audio, rate = soundfile.read(file, dtype="float32")
        return resample(mono(audio), rate)

    @staticmethod
    def _bytes(data):
        audio, rate = soundfile.read(io.BytesIO(data), dtype="float32")
        return resample(mono(audio), rate)


def backgrounds(inputs, cache):
    """Every background file decoded once into `cache` (16 kHz f32 .npy), split
    into music (>= 20 s) and effects (shorter)."""
    cache.mkdir(parents=True, exist_ok=True)
    music, effects = [], []
    for spec in inputs:
        for file in sorted(Path(spec).rglob("*")):
            if file.suffix.lower() not in AUDIO:
                continue
            target = cache / (hashlib.sha1(str(file.resolve()).encode()).hexdigest() + ".npy")
            if target.exists():
                audio = np.load(target)
            else:
                try:
                    audio = decode_ffmpeg(file)
                except subprocess.CalledProcessError:
                    continue
                np.save(target, audio)
            if len(audio) < RATE // 10:
                continue
            (music if len(audio) >= 20 * RATE else effects).append((file.name, audio))
    if not music or not effects:
        sys.exit(f"need music and effects: {len(music)} music, {len(effects)} effects")
    return music, effects


def bed(rng, length, music, effects, kind, level_db):
    """`length` samples of game audio at `level_db` dBFS RMS: music, or music
    plus effects every 0.5-6 s."""
    out = np.zeros(length, dtype=np.float32)
    name, track = rng.choice(music)
    start = rng.randrange(len(track))
    tiled = np.tile(track, length // len(track) + 2)[start : start + length]
    out += gain_to(tiled, level_db)
    used = [name]
    if kind == "music+effects":
        at = 0
        while True:
            at += int(rng.uniform(0.5, 6.0) * RATE)
            if at >= length:
                break
            effect_name, effect = rng.choice(effects)
            piece = gain_to(effect, level_db + rng.uniform(-3, 6))[: length - at]
            out[at : at + len(piece)] += piece
            used.append(effect_name)
    return out, used


def mix(args):
    rng = random.Random(args.seed)
    speech = Speech(args.speech)
    order = list(range(len(speech.items)))
    rng.shuffle(order)
    music, effects = backgrounds(args.background, Path(args.out) / ".cache")
    out_dir = Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)
    files = int(np.ceil(args.hours * 3600 / FILE_SECONDS))
    cursor = 0
    totals = {"files": files, "seconds": 0.0, "speechSeconds": 0.0, "utterances": 0, "scenes": {}}
    for index in range(files):
        target = FILE_SECONDS * RATE
        audio = np.zeros(target, dtype=np.float32)
        at = 0
        while at < target:
            if rng.random() < 0.8:
                scene_len = min(int(rng.uniform(30, 180) * RATE), target - at)
                kind = rng.choices(["quiet", "music", "music+effects"], weights=[25, 50, 25])[0]
                scene = np.zeros(scene_len, dtype=np.float32)
                position, speech_level = int(rng.uniform(0.2, 1.5) * RATE), -26.0
                while position < scene_len:
                    if cursor >= len(order):
                        rng.shuffle(order)
                        cursor = 0
                    source, ident, load = speech.items[order[cursor]]
                    cursor += 1
                    utterance = gain_to(load(), speech_level + rng.uniform(-6, 6))[: scene_len - position]
                    scene[position : position + len(utterance)] += utterance
                    totals["speechSeconds"] += len(utterance) / RATE
                    totals["utterances"] += 1
                    gap = rng.uniform(4, 8) if rng.random() < 0.125 else rng.uniform(0.2, 2.5)
                    position += len(utterance) + int(gap * RATE)
                if kind != "quiet":
                    snr = rng.uniform(5, 20)
                    background, _ = bed(rng, scene_len, music, effects, kind, speech_level - snr)
                    scene += background
            else:
                scene_len = min(int(rng.uniform(30, 120) * RATE), target - at)
                kind = "game-only"
                scene, _ = bed(rng, scene_len, music, effects, "music+effects", rng.uniform(-30, -18))
            audio[at : at + scene_len] = scene
            totals["scenes"][kind] = totals["scenes"].get(kind, 0) + 1
            at += scene_len
        audio += (np.random.default_rng(args.seed + index).standard_normal(target).astype(np.float32) * 10 ** (-65 / 20))
        peak = float(np.max(np.abs(audio)))
        if peak > 0.99:
            audio *= 0.99 / peak
        soundfile.write(out_dir / f"{args.label}-{index:02}.wav", audio, RATE, subtype="PCM_16")
        totals["seconds"] += target / RATE
        print(f"wrote {args.label}-{index:02}.wav", file=sys.stderr)
    manifest = {
        "label": args.label,
        "seed": args.seed,
        "hours": round(totals["seconds"] / 3600, 2),
        "speechHours": round(totals["speechSeconds"] / 3600, 2),
        "utterances": totals["utterances"],
        "scenes": totals["scenes"],
        "speech": [Path(spec).name for spec in args.speech],
        "speechPool": len(speech.items),
        "backgroundFiles": {"music": len(music), "effects": len(effects)},
    }
    print(json.dumps(manifest))


def positives(args):
    """Each built-in speaker says each phrase, alone and followed by a short
    command, 24 kHz from the model, written as 16 kHz WAVs under `out`
    (the harness counts a clip under a `positives` directory as a wake)."""
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    commands = {
        "en": ["", "what's on my board today?", "move NEW-78 to done."],
        "de": ["", "was steht heute an?", "schieb NEW-78 auf erledigt."],
    }[args.language]
    written = 0
    for phrase in args.phrase:
        for speaker in args.speaker:
            for number, command in enumerate(commands):
                text = f"{phrase}, {command}" if command else f"{phrase}."
                slug = phrase.lower().replace(" ", "-")
                raw = out / f".{slug}-{speaker}-{number}.24k.wav"
                subprocess.run(
                    [args.crispasr, "--backend", "qwen3-tts-customvoice", "-m", args.model, "--codec-model", args.codec,
                     "--voice", speaker, "-tl", args.language, "--seed", str(args.seed + number), "--tts", text,
                     "--tts-output", str(raw)],
                    check=True,
                    capture_output=True,
                )
                audio, rate = soundfile.read(raw, dtype="float32")
                soundfile.write(out / f"{slug}-{args.language}-{speaker}-{number}.wav", resample(mono(audio), rate), RATE, subtype="PCM_16")
                raw.unlink()
                written += 1
    print(json.dumps({"language": args.language, "phrases": args.phrase, "speakers": args.speaker, "clips": written}))


def main():
    parser = argparse.ArgumentParser()
    commands = parser.add_subparsers(dest="command", required=True)
    m = commands.add_parser("mix")
    m.add_argument("--speech", nargs="+", required=True)
    m.add_argument("--background", nargs="+", required=True)
    m.add_argument("--out", required=True)
    m.add_argument("--hours", type=float, default=8.5)
    m.add_argument("--seed", type=int, default=56)
    m.add_argument("--label", default="mix")
    p = commands.add_parser("positives")
    p.add_argument("--crispasr", required=True)
    p.add_argument("--model", required=True)
    p.add_argument("--codec", required=True)
    p.add_argument("--out", required=True)
    p.add_argument("--language", choices=["en", "de"], required=True)
    p.add_argument("--phrase", action="append", required=True)
    p.add_argument("--speaker", action="append", default=None)
    p.add_argument("--seed", type=int, default=7)
    args = parser.parse_args()
    if args.command == "mix":
        mix(args)
    else:
        args.speaker = args.speaker or ["aiden", "ryan", "vivian", "serena", "dylan", "eric", "uncle_fu", "ono_anna", "sohee"]
        positives(args)


if __name__ == "__main__":
    main()
