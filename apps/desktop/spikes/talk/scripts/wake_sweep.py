#!/usr/bin/env python3
"""#NEW-56: replay the wake match over the harness's check transcripts at
other edit tolerances, without re-running the speech model.

    python3 -I apps/desktop/spikes/talk/scripts/wake_sweep.py <results-dir> [ratio...]

Reads what `cargo run -p voice --example wake -- ... --checks-out F` wrote:
`checks-<lang>-<slug>.jsonl` (every negative check) and `<lang>-<slug>.jsonl`
(the harness lines; the positives line carries every check of a missed clip
and the check that woke each other one). For each tolerance it prints the
false accepts per 8 h and the true accepts, per language and phrase.

The match is `voice::wake::wake_distance` re-implemented (words = runs of
letters and digits, lowercased, accents folded, an apostrophe joining; the
phrase's letters against the first 1 .. n+2 words joined). It is a sweep, not
the number of record: the chosen tolerance is measured again with the Rust
harness.
"""

import json
import math
import sys
from pathlib import Path

FOLD = str.maketrans({"ä": "a", "á": "a", "à": "a", "â": "a", "å": "a", "ö": "o", "ó": "o", "ò": "o", "ô": "o",
                      "ü": "u", "ú": "u", "ù": "u", "û": "u", "é": "e", "è": "e", "ê": "e", "ë": "e",
                      "í": "i", "ì": "i", "î": "i", "ï": "i", "ç": "c", "ñ": "n"})


def words(text):
    out, current = [], None
    for ch in text:
        if ch.isalnum():
            current = (current or "") + ch.lower().replace("ß", "ss").translate(FOLD)
        elif ch in "'’" and current is not None:
            continue
        elif current is not None:
            out.append(current)
            current = None
    if current is not None:
        out.append(current)
    return out


def levenshtein(a, b):
    row = list(range(len(b) + 1))
    for i, ca in enumerate(a):
        diagonal, row[0] = row[0], i + 1
        for j, cb in enumerate(b):
            above = row[j + 1]
            row[j + 1] = min(above + 1, row[j] + 1, diagonal + (ca != cb))
            diagonal = above
    return row[len(b)]


def distance(phrase, heard):
    phrase_words = words(phrase)
    key = "".join(phrase_words)
    best, candidate = None, ""
    for word in words(heard)[: len(phrase_words) + 2]:
        candidate += word
        d = levenshtein(candidate, key)
        best = d if best is None or d < best else best
    return best


def allowed(phrase, ratio):
    return math.floor(len("".join(words(phrase))) * ratio + 1e-9)


def main():
    results = Path(sys.argv[1])
    ratios = [float(r) for r in sys.argv[2:]] or [0.25, 0.3, 1 / 3, 0.35, 0.4]
    for line_file in sorted(results.glob("[a-z][a-z]-*.jsonl")):
        lang, slug = line_file.stem.split("-", 1)
        lines = [json.loads(line) for line in line_file.read_text().splitlines() if line.strip()]
        negatives = next((line for line in lines if line["run"] == "negatives"), None)
        positives = next((line for line in lines if line["run"] == "positives"), None)
        if not negatives:
            continue
        phrase = negatives["phrase"]
        checks_file = results / f"checks-{lang}-{slug}.jsonl"
        checks = [json.loads(line)["heard"] for line in checks_file.read_text().splitlines()] if checks_file.exists() else []
        hours = negatives["audioSeconds"] / 3600
        for ratio in ratios:
            limit = allowed(phrase, ratio)
            false = sum(1 for heard in checks if (d := distance(phrase, heard)) is not None and d <= limit)
            row = {"language": lang, "phrase": phrase, "ratio": round(ratio, 3), "allowedEdits": limit, "checks": len(checks),
                   "falseAccepts": false, "falseAcceptsPer8h": round(false * 8 / hours, 2)}
            if positives:
                woke = len(positives["heard"])
                woke += sum(1 for miss in positives["misses"] if any((d := distance(phrase, heard)) is not None and d <= limit for heard in miss["checks"]))
                row["trueAccepts"] = f"{woke}/{positives['clips']}"
            print(json.dumps(row))


if __name__ == "__main__":
    main()
