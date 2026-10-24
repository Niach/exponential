#!/usr/bin/env python3
"""NEW-52: fold the harness's JSON lines under out/ into numbers.json.

    scripts/collect.py <machine-label> [out-dir] > numbers.json

Every engine row keeps the raw per-clip lines' medians; WER comes from
scripts/wer.py over the same clips. Fields that a human measured by hand
(VRAM from nvidia-smi, the llama-tts timings) are read from out/manual.json
when it exists, so a re-run on another machine starts from an empty file.
"""
import json
import os
import platform
import statistics
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
OUT = sys.argv[2] if len(sys.argv) > 2 else os.path.join(ROOT, "out")
PHRASES = os.path.join(ROOT, "data", "phrases.json")


def rows(name):
    path = os.path.join(OUT, name)
    if not os.path.exists(path):
        return []
    return [json.loads(l) for l in open(path) if l.strip()]


def median(values):
    values = [v for v in values if v is not None]
    return round(statistics.median(values), 1) if values else None


def wer(name, label):
    path = os.path.join(OUT, name)
    if not os.path.exists(path):
        return None
    out = subprocess.run(
        [sys.executable, os.path.join(HERE, "wer.py"), PHRASES, path, label],
        capture_output=True, text=True, check=True,
    ).stdout
    return json.loads(out.strip().splitlines()[-1])


def asr(engine, file, label):
    r = rows(file)
    if not r:
        return None
    return {
        "engine": engine,
        "clips": file.replace(".jsonl", ""),
        "wer": (wer(file, label) or {}).get("wer"),
        "first_partial_audio_ms_median": median([x.get("first_partial_audio_ms") for x in r]),
        "tail_ms_median": median([x.get("tail_ms") for x in r]),
        "tail_ms_max": max((x.get("tail_ms") or 0) for x in r),
        "rtf_median": round(statistics.median(x["rtf"] for x in r), 3) if "rtf" in r[0] else None,
        "load_ms": r[0].get("load_ms"),
        "wall_ms_median": median([x.get("wall_ms") for x in r]),
    }


def tts(engine, file):
    r = rows(file)
    if not r:
        return None
    return {
        "engine": engine,
        "clips": len(r),
        "ttfa_ms_median": median([x.get("ttfa_ms") for x in r]),
        "wall_ms_median": median([x.get("wall_ms") for x in r]),
        "rtf_median": round(statistics.median(x["rtf"] for x in r), 3) if "rtf" in r[0] else None,
        "load_ms": r[0].get("load_ms"),
    }


def vad(file):
    r = rows(file)
    lags = [s["eou_lag_ms"] for x in r for s in x["segments"]]
    return {
        "engine": "silero-vad (sherpa-onnx)",
        "clips": len(r),
        "segments": len(lags),
        "eou_lag_ms_median": median(lags),
        "compute_ms_per_clip_median": median([x["compute_ms"] for x in r]),
    } if r else None


def kws(file):
    r = rows(file)
    return {
        "engine": "sherpa-onnx kws zipformer zh-en 3M",
        "clips": len(r),
        "hits": [(x["wav"].rsplit("/", 1)[-1], h["keyword"], h["at_ms"]) for x in r for h in x["hits"]],
        "rtf_median": round(statistics.median(x["rtf"] for x in r), 3),
    } if r else None


manual = json.load(open(os.path.join(OUT, "manual.json"))) if os.path.exists(os.path.join(OUT, "manual.json")) else {}

numbers = {
    "$comment": "NEW-52 exponentialTalk spike numbers. Synthetic clips: the 20 phrases of data/phrases.json spoken by Kokoro (en) and Qwen3-TTS 1.7B (de + en); no human recordings yet (scripts/record.sh). WER is scored against the spoken form; identifiers like 'E X P twelve forty two' count word by word, which penalises engines that normalise them to 'exp1242'.",
    "machine": sys.argv[1] if len(sys.argv) > 1 else platform.node(),
    "platform": platform.platform(),
    "asr": [x for x in [
        asr("nemotron-3.5-asr-streaming-0.6b Q8_0, transcribe.cpp CUDA", "nemotron-kokoro-en.jsonl", "nemotron"),
        asr("nemotron-3.5-asr-streaming-0.6b Q8_0, transcribe.cpp CUDA", "nemotron-qwen17b-en.jsonl", "nemotron"),
        asr("nemotron-3.5-asr-streaming-0.6b Q8_0, transcribe.cpp CUDA", "nemotron-qwen17b-de.jsonl", "nemotron"),
        asr("whistle (cactus-rs 0.2.1), CPU", "whistle-kokoro-en.jsonl", "whistle"),
        asr("whistle (cactus-rs 0.2.1), CPU", "whistle-qwen17b-en.jsonl", "whistle"),
        asr("whistle (cactus-rs 0.2.1), CPU", "whistle-qwen17b-de.jsonl", "whistle"),
        asr("zipformer streaming en-2023-06-26 (sherpa-onnx), CPU 4 threads", "zipformer-en-kokoro.jsonl", "zipformer"),
        asr("zipformer streaming en-2023-06-26 (sherpa-onnx), CPU 4 threads", "zipformer-qwen17b-en.jsonl", "zipformer"),
    ] if x],
    "tts": [x for x in [tts("kokoro v1.0 (sherpa-onnx 2.0, lexicon, no espeak-ng), CPU 4 threads", "kokoro-en.jsonl")] if x],
    "vad": [x for x in [vad("vad-kokoro.jsonl")] if x],
    "kws": [x for x in [kws("kws-kokoro.jsonl")] if x],
    "manual": manual,
}
print(json.dumps(numbers, indent=2, ensure_ascii=False))
