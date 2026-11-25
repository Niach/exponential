"""NEW-129: the German part of "Hey Claude": German speech in German voices and German pronunciation.

    python german.py accent <config.yaml>               (livekit-wakeword's env)
    python german.py voices <voices dir> <output dir>   (piper-tts + scipy)

German voices read "Hey Claude" letter by letter (haɪ klaʊdə), which nobody says, so the
phrase is given as phonemes. `accent` = livekit-wakeword's own multi-speaker generator
(904 English speakers) speaking German phonemes; `voices` = Piper's CC0 German voices
(Thorsten ×3, Kerstin). Karlsson, Ramona, Eva K and MLS are never used here: they are the
check's unseen voices. Clips join livekit-wakeword's after its own numbers; a rerun goes on
after the clips that exist. STAGED_WORKERS processes.
"""

from __future__ import annotations

import multiprocessing
import os
import random
import sys
import wave
from pathlib import Path

HEY = ["hˈɛɪ", "hˈeɪ", "hˈeː"]
CLAUDE = ["klˈoːt", "klˈoːd", "klˈoʊd", "klˈɔːt", "klˈɔːd"]
NEAR = ["klˈaʊt", "klˈaʊd", "klˈaʊdiːˌɑː", "klˈaʊdɪˌoː", "klˈaʊs", "klˈuːk", "klˈoː", "klˈoːn", "klˈoːstɜ", "ʃlˈoːt", "brˈoːt", "lˈoːt", "ɡlˈoːbʊs", "klˈaɪt", "krˈaʊt", "lˈaʊt"]
LOOSE = [f"[[{hey}]]" for hey in HEY] + [f"[[{name}]]" for name in CLAUDE] + [f"[[ˈoːkeɪ {name}]]" for name in CLAUDE]
WITH_CLAUDE = [
    "Ich habe {} gefragt.", "{} hat den Fehler gefunden.", "Kannst du das {} schicken?", "Das war die Idee von {}.", "Gestern hat {} den Bericht geschrieben.",
    "Frag doch mal {}.", "Mein Onkel {} wohnt in Paris.", "Danke, {}.", "Wir arbeiten seit Wochen mit {}.", "Ist {} heute da?", "{} meint, es dauert eine Stunde.",
    "Dann hat {} geantwortet.", "Guten Morgen, {}.", "Ich glaube, {} hat recht.", "Das Modell heißt {}.",
]
SENTENCES = [
    "Kannst du mir das bitte noch einmal erklären?", "Morgen soll es den ganzen Tag regnen.", "Hey, hast du schon gegessen?", "Wir treffen uns um halb neun am Bahnhof.",
    "Der Zug hat leider zwanzig Minuten Verspätung.", "Hey, mach bitte die Tür zu.", "Ich muss heute noch einkaufen gehen.", "Die Wolken ziehen langsam nach Osten.",
    "Hat jemand meinen Schlüssel gesehen?", "Das Kloster steht seit achthundert Jahren.", "Hey, das war eine gute Idee.", "Er hat den ganzen Abend Klavier gespielt.",
    "Die Rechnung kommt nächste Woche.", "Kaum war er da, ging er schon wieder.", "Hey, komm mal kurz her.", "Das Brot ist noch warm.", "Sie wohnt jetzt in Hamburg.",
    "Wo hast du das Auto geparkt?", "Ich hätte gern einen Kaffee und eine Breze.", "Hey, wie lange dauert das noch?", "Am Wochenende fahren wir in die Berge.",
    "Der Schlot raucht schon wieder.", "Das ist mir ehrlich gesagt zu laut.", "Hallo zusammen, können wir anfangen?", "Die Katze schläft auf dem Sofa.",
    "Hey, pass auf, da kommt ein Fahrrad.", "Ich glaube, wir haben uns verlaufen.", "Das Treffen wurde auf Dienstag verschoben.", "Klar, das kann ich machen.",
    "Okay, dann sehen wir uns später.", "Hey, klau mir nicht die Pommes.", "Er hat Lotto gespielt und verloren.", "Bitte speichere die Datei im Ordner.",
    "Wir brauchen mehr Zeit für die Tests.", "Hey Leute, hört mal kurz zu.", "Der Vertrag hat eine neue Klausel.", "Das Kleid ist rot und blau.", "Wer hat das Licht angelassen?",
    "Heute kocht mein Bruder.", "Hey Klaus, ruf mich später an.",
]
# split, first clip, clips of each half (`accent` first, then `voices`)
SPLITS = [("positive_train", 25000, 3000), ("negative_train", 25000, 3000), ("positive_test", 5000, 500), ("negative_test", 5000, 500)]
VOICES = ["de_DE-thorsten-medium", "de_DE-thorsten-high", "de_DE-thorsten_emotional-medium", "de_DE-kerstin-low"]


def text_of(kind: str, rng: random.Random) -> str:
    if kind == "positive":
        return f"[[{rng.choice(HEY)} {rng.choice(CLAUDE)}]]"
    roll = rng.random()
    if roll < 0.45:
        return f"[[{rng.choice(HEY)} {rng.choice(NEAR)}]]"
    if roll < 0.55:
        return rng.choice(LOOSE)
    if roll < 0.75:
        return rng.choice(WITH_CLAUDE).format(f"[[{rng.choice(CLAUDE)}]]")
    return rng.choice(SENTENCES)


def trimmed(audio):
    import numpy as np

    frame = 320
    levels = np.abs(audio[: len(audio) // frame * frame]).reshape(-1, frame).max(axis=1)
    loud = np.nonzero(levels > levels.max() * 0.02)[0]
    return audio[loud[0] * frame : (loud[-1] + 1) * frame]


def _work_voices(job: tuple[str, str, str, int, int]) -> None:
    voices_dir, out, split, first, last = job
    missing = [i for i in range(first, last) if not (Path(out) / split / f"clip_{i:06d}.wav").exists()]
    if not missing:
        return
    import numpy as np
    from piper import PiperVoice, SynthesisConfig
    from scipy.signal import resample_poly

    voices = [PiperVoice.load(str(Path(voices_dir) / f"{name}.onnx")) for name in VOICES]
    kind = split.split("_")[0]
    for i in missing:
        rng = random.Random(f"{split}-{i}")
        voice = rng.choice(voices)
        speaker = rng.randrange(voice.config.num_speakers) if voice.config.num_speakers > 1 else None
        config = SynthesisConfig(speaker_id=speaker, length_scale=rng.uniform(0.75, 1.35), noise_scale=rng.uniform(0.5, 1.0), noise_w_scale=rng.uniform(0.5, 1.0))
        audio = np.concatenate([chunk.audio_float_array for chunk in voice.synthesize(text_of(kind, rng), syn_config=config)])
        if voice.config.sample_rate == 22050:
            audio = resample_poly(audio, 320, 441)
        audio = trimmed(audio)
        path = Path(out) / split / f"clip_{i:06d}.wav"
        partial = path.with_suffix(".partial")
        with wave.open(str(partial), "wb") as file:
            file.setframerate(16000)
            file.setsampwidth(2)
            file.setnchannels(1)
            file.writeframes((np.clip(audio, -1, 1) * 32767).astype(np.int16).tobytes())
        partial.rename(path)


def _work_accent(job: tuple[str, str, int, int]) -> None:
    path, split, first, last = job
    import re

    import torch
    from livekit.wakeword.config import load_config
    from livekit.wakeword.data.piper import synthesis

    torch.set_num_threads(int(os.environ.get("STAGED_THREADS", "2")))
    config = load_config(path)
    out = config.model_output_dir / split
    start = first
    while start < last and (out / f"clip_{start:06d}.wav").exists():
        start += 1
    if start >= last:
        return
    espeak = synthesis._espeak_phonemize

    def phonemize(text: str, voice: str = "en-us") -> str:
        parts = re.split(r"\[\[(.*?)\]\]", text)  # [[phonemes]] as they are, the rest as German
        return " ".join(part if i % 2 else espeak(part, "de") for i, part in enumerate(parts) if part.strip(" ."))

    synthesis._espeak_phonemize = phonemize
    kind = split.split("_")[0]
    texts = [text_of(kind, random.Random(f"{split}-{i}")) for i in range(first, last)]
    # generate_samples cycles `text` and the speaker pairs from clip 0 and skips to start_index.
    synthesis.generate_samples(
        text=texts[-(first % len(texts)) or len(texts) :] + texts[: -(first % len(texts)) or len(texts)],
        output_dir=out,
        max_samples=last,
        model=config.piper_checkpoint_path,
        batch_size=config.tts_batch_size,
        slerp_weights=config.slerp_weights,
        length_scales=config.length_scales,
        noise_scales=config.noise_scales,
        noise_scale_ws=config.noise_scale_ws,
        start_index=start,
    )


def main() -> None:
    mode = sys.argv[1]
    jobs: list = []
    for split, first, half in SPLITS:
        if mode == "accent":
            jobs += [(sys.argv[2], split, i, min(i + 250, first + half)) for i in range(first, first + half, 250)]
        else:
            first += half
            (Path(sys.argv[3]) / split).mkdir(parents=True, exist_ok=True)
            jobs += [(sys.argv[2], sys.argv[3], split, i, min(i + 250, first + half)) for i in range(first, first + half, 250)]
    os.environ.setdefault("OMP_NUM_THREADS", "2")
    with multiprocessing.get_context("spawn").Pool(int(os.environ.get("STAGED_WORKERS", "4"))) as pool:
        pool.map(_work_accent if mode == "accent" else _work_voices, jobs, chunksize=1)


if __name__ == "__main__":
    main()
