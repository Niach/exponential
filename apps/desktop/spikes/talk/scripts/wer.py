#!/usr/bin/env python3
"""NEW-52: word error rate of one engine's transcripts against data/phrases.json.

    scripts/wer.py data/phrases.json <transcripts.jsonl> [engine-label]

A transcripts line is `{"id": "en-01", "text": "..."}` (or the harness's ASR
row with `wav` naming the clip, `<id>.wav`). Normalisation is the one the
dispatcher would apply before matching: lower-case, punctuation dropped,
hyphens and the space inside an identifier removed (`EXP-1242`, `exp 1242`
and `EXP1242` are the same token), digits kept.
"""
import json
import re
import sys


def normalise(text: str) -> list[str]:
    text = text.lower()
    text = re.sub(r"\b([a-z]{2,4})[\s-]+(\d{1,5})\b", r"\1\2", text)   # exp-1242 → exp1242
    text = re.sub(r"[^\w\s]", " ", text)
    return text.split()


def edit_distance(a: list[str], b: list[str]) -> int:
    prev = list(range(len(b) + 1))
    for i, x in enumerate(a, 1):
        cur = [i]
        for j, y in enumerate(b, 1):
            cur.append(min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + (x != y)))
        prev = cur
    return prev[-1]


def main() -> None:
    phrases = {p["id"]: p for p in json.load(open(sys.argv[1]))["phrases"]}
    label = sys.argv[3] if len(sys.argv) > 3 else sys.argv[2]
    errors = words = 0
    per_lang: dict[str, list[int]] = {}
    rows = []
    for line in open(sys.argv[2]):
        line = line.strip()
        if not line:
            continue
        row = json.loads(line)
        pid = row.get("id") or row["wav"].rsplit("/", 1)[-1].removesuffix(".wav")
        ref = normalise(phrases[pid].get("spoken") or phrases[pid]["ref"])
        hyp = normalise(row.get("text", ""))
        d = edit_distance(ref, hyp)
        errors += d
        words += len(ref)
        lang = phrases[pid]["lang"]
        per_lang.setdefault(lang, [0, 0])
        per_lang[lang][0] += d
        per_lang[lang][1] += len(ref)
        rows.append({"id": pid, "lang": lang, "errors": d, "words": len(ref), "hyp": " ".join(hyp)})
    summary = {
        "engine": label,
        "wer": round(100 * errors / max(words, 1), 1),
        "per_lang": {k: round(100 * v[0] / max(v[1], 1), 1) for k, v in per_lang.items()},
        "clips": len(rows),
    }
    print(json.dumps(summary))
    for r in rows:
        print(json.dumps(r), file=sys.stderr)


if __name__ == "__main__":
    main()
