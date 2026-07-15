# Live-action footage (Seedance picks)

Media in this directory is **gitignored** — only this README is committed.
Full pipeline + generation prompts: `docs/storyboard-launch-spot.md` §2.

Expected files (drop the picked takes here, then flip `ready: true` in
`src/spot/footage.tsx` and tune each entry's `trimBeforeSec` to the take):

| File | Source | Used by |
|---|---|---|
| `cafe-a.mp4` | Seedance clip A — chaos → quiet, 15s multi-shot, 16:9 1080p | segments A (trim ~0–4s) + B (trim ~5.5–9.1s) |
| `cafe-c.mp4` | Seedance clip C — coffee payoff, 10s (uses clip-A reference stills) | segment C |
| `music.mp3` | Music bed, ≥36s (not Seedance) | whole spot (storyboard §5) |
| `keys.mp3` | Keyboard foley loop | under the UI act (storyboard §5) |

Raw takes go to `ref/footage-takes/` (also gitignored) for side-by-side picking.
Reject any take with readable screen content — the recreated UI is the only
screen truth in the film.
