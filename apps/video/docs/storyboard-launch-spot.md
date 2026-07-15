# Storyboard — "Launch Spot" (Before the Coffee)

The 36s launch **ad**: Seedance-generated live-action café footage + recut UI beats from
the `ShipsItsOwnIssues` film. 1920×1080 @ 30fps, **1080 frames (36.0s)**.
Remotion composition id: `LaunchSpot` (`src/spot/`). The 50s `ShipsItsOwnIssues` film keeps
living as the full product walkthrough; this cut is the feed/ad version.

Story: a developer sits down in a loud café, starts typing, the world goes quiet — the agent
does the work — the release is shipped before the coffee lands on the table.
Theme: people + efficiency through AI. More time to live, less hassle.

---

## 0. Hard rules

1. **Live action carries emotion, Remotion carries product truth.** Seedance never renders
   the app — AI-generated screens are garbled text next to our pixel-perfect recreated UI.
   In every live shot the MacBook screen is angled away, out of focus, or a soft glow.
2. **UI beats run at ≥1.35× camera, hero beats at 1.6–2.3×** — the #1 fix over the 50s film
   (which ran information beats as low as 1.15×). Primary text must hit ≥18px effective.
3. **No lower-third captions.** Big type only: full-frame type cards between UI beats
   (they double as transitions and cover the shell/camera switches) and PUNCH-size type
   composited over the live footage.
4. Everything Remotion-side derives from `useCurrentFrame()` — same determinism rules,
   springs, EASE and CLI grammar as the ships film (storyboard-ships-its-own-issues.md §0).
5. Reuse `src/ships/surfaces/*` untouched. The spot has its own timeline
   (`src/spot/timeline.ts`) and its own shells (`src/spot/Spot.tsx`); it never imports
   `ships/scenes/timeline.ts` frame constants.

---

## 1. Scene table (30fps · 1080 frames)

| # | Segment | from | frames | ends | Source | Big type |
|---|---------|------|--------|------|--------|----------|
| A | LIVE — café chaos: sit down, open MacBook, loud | 0 | 120 | 120 | Seedance clip A (trim 0.8–4.8s) | `9:04 AM. One fix to ship.` (f24–96) |
| B | LIVE — fingers hit keys, world goes quiet, push-in on glowing screen | 120 | 108 | 228 | Seedance clip A (trim 4.8–8.4s) | — (the audio duck carries it) |
| — | crossfade B → UI (footage fades out over the board) | 219 | 9 | 228 | — | — |
| U1 | UI — ISSUE PICK: board list → cursor click → detail slides in → push onto ▷ Start coding → click → In Progress + Coding-now pill (1.6→2.0×) | 228 | 117 | 345 | Remotion | — |
| C1 | TYPE CARD (covers the camera cut + dock opening) | 345 | 48 | 393 | Remotion | `Your issue tracker.` / `Writing code.` |
| U2 | UI — dock run + phone PiP, session finishes → PR #214 (1.8×) | 393 | 165 | 558 | Remotion | caption `Steer from your phone.` (f444–538) |
| U3 | UI — HARD CUT → releases list (dock already printing) → drill f592 → waves → Shipped pill (1.26×) | 558 | 192 | 750 | Remotion | caption `Or ship a whole release.` (f606–700, while the lanes pop) |
| C3 | TYPE CARD — the payoff word stands alone (UI unmounts beneath at f762; the card lifts OVER the cup footage, out f806) | 750 | 48 | 798 | Remotion | `Shipped.` |
| C | LIVE — coffee lands next to the laptop, lean back, nod thanks | 798 | 192 | 990 | Seedance clip C | `Before the coffee.` (f828–950) |
| O | OUTRO — brand card (compressed ships S12) | 990 | 90 | 1080 | Remotion | `Exponential` / `Shipping faster than your barista.` / URL |

Beat-to-timecode: A 0:00–4.0 · B 4.0–7.6 · U1 7.6–11.5 · C1 11.5–13.1 · U2 13.1–18.6 ·
U3 18.6–25.0 · C3 25.0–26.6 · C 26.6–33.0 · O 33.0–36.0.

Text inventory (r2): TWO type cards (C1 hook, C3 `Shipped.`), TWO punches on the live
bookends, TWO in-beat captions (steer during the phone PiP, release while the lanes
pop). U1's cursor choreography (pick → detail → Start coding click) is the spot's only
cursor. The arc reads as one sentence: *one fix to ship → (your issue tracker, writing
code — steer from your phone — or ship a whole release) → Shipped. → Before the coffee.*

---

## 2. Live action — Seedance pipeline

**Two generations** (Seedance 2.0: 15s max per clip, native synchronized audio, multi-shot
via timestamp markers, up to 9 reference images for character consistency).

### 2.1 Clip A — chaos → quiet (15s, one multi-shot generation)

One generation so the actor/lighting can't drift across the story hinge.

```
A man in his early 30s, short dark hair, stubble, olive-green knit sweater,
at a small window table in a busy specialty coffee shop, morning light.
[0-5s] Medium shot: he sits down and opens a silver laptop; the screen
faces away from camera. Around him the café is crowded and loud — baristas
steaming milk, chatter, cups clattering.
[5-9s] Extreme close-up: his fingers start typing on the keyboard, calm
and deliberate. Busy café bokeh behind.
[9-15s] Slow over-the-shoulder push-in toward the laptop; the screen is a
soft out-of-focus glow, the café blurs and darkens at the edges of frame.
Cinematic, shallow depth of field, warm morning tones, 35mm look.
Audio: loud café ambience — overlapping conversations, espresso machine
hissing, ceramic clatter — that gradually fades from 5s on until only
crisp mechanical keyboard keystrokes remain.
```

### 2.2 Clip C — payoff (10s; attach 2–3 stills of the actor from clip A as reference images)

```
Same man, early 30s, short dark hair, stubble, olive-green knit sweater,
at a window table in a café, silver laptop open, morning light.
[0-4s] Close-up: a barista's hand lowers a flat white in a ceramic cup
onto the wooden table beside the laptop. Gentle clink.
[4-10s] Medium shot: he leans back, half-closes the laptop, exhales with a
small satisfied smile, nods a friendly thank-you toward the barista
off-screen, picks up the cup.
Cinematic, warm tones, shallow depth of field, 35mm look.
Audio: soft café ambience at low volume, cup set gently on wood, a quiet
"thanks", relaxed.
```

### 2.3 Take workflow

> **Picked takes (2026-07-13):** `cafe-a.mp4` 11.0s (sit ~0–2s · typing ~2–6.5s ·
> screen push ~6.5–11s) and `cafe-c.mp4` 10.1s — both **960×960** @24fps. The square
> frame is cover-cropped to 16:9 with a per-shot vertical anchor (`objectPosY` in the
> manifest); the seg-A boundary rides the take's own sit→typing cut at ~2s. The clip-A
> screen-push shot shows a garbled dark editor — acceptable ONLY because it's the
> crossfade bridge into the real UI (mostly covered by the fade). If regenerating:
> ask for 16:9 1080p to skip the crop dance and win back resolution (960→1920 is a 2× upscale).

- Generate **2–3 takes per clip**, 16:9 1080p. Park raw takes in `ref/footage-takes/`
  (gitignored territory — never committed).
- Pick the best take of each and save as:
  - `public/footage/cafe-a.mp4` — clip A (used twice: segment A trims ~0–4s, segment B
    trims ~5.5–9.1s; adjust `trimBeforeSec` in `src/spot/footage.tsx` to the take)
  - `public/footage/cafe-c.mp4` — clip C
- Flip `ready: true` on each entry in the `FOOTAGE` manifest (`src/spot/footage.tsx`).
  Until then the spot renders **placeholder slates** with the shot description, so the
  full cut is watchable/reviewable before any footage exists.
- `public/footage/` is gitignored (heavy binaries). `public/footage/README.md` documents
  the expected files.

### 2.4 Screen rule check per shot

- A [0-5s]: screen faces away ✓ · A [5-9s]: keyboard macro, no screen ✓ ·
  A [9-15s]: screen = bokeh glow ✓ (this is the crossfade bridge — the blur is what makes
  the cut to the crisp recreated UI land) · C: laptop half-closed / at an angle ✓.
  If a take shows readable screen content, it's a rejected take.

---

## 3. UI beats (recut, not rebuilt)

All three beats reuse `ships/surfaces/*` + `ships/fixtures.ts` verbatim — same world
(EXP-142, PR #214, release v0.12, PR #219). New spot-local schedules in `src/spot/timeline.ts`.

### U1 · Issue pick (f228–345, 3.9s) — camera 1.6→2.0×

**No terminal in the opening beat, and this one is interactive** (review r2) — the
crossfade out of the bokeh screen lands on the BOARD (camera `{1.6, 600, 375}`,
window-left pinned, center empty state "Nothing open"). The spot's only cursor: glide
to EXP-142 f230–248, hover, **click f252** → tab pops, detail slides in f254, camera
eases to the detail `{1.6, 910, 398}` (title + description + properties; RELEASE
🚀 v0.12 plants act two) → camera pushes onto **▷ Start coding** `{2.0, 1088, 270}`
(right+top edges pinned), hover f306, **click f318** → status flips ⏱ In Progress f321,
Coding-now pill pops f330 (no dialog — ad, not tutorial). Dock stays a collapsed strip
and opens under card C1 (f380). Effective title: 20px × 1.6 = **32px**.

### U2 · Dock run + phone PiP (f393–555, 5.4s) — camera 1.8×

Same shell, same session **continuing** — the terminal's FIRST appearance: camera cut
(under card C1) to `{1.8, 553, 680}` — window bottom pinned to the frame bottom AND the
terminal line-starts pinned to the left frame edge (x = 24 + 952/s, the ships-S6 rule:
never crop the payoff line). Sidebar board fully visible left, dock = the lower 40%,
static camera (the phone is the motion). Session events 0–3 pre-rendered (f300–330,
invisible until the dock opens); typecheck f408, tests f438, spinner f462, push types
f480, `exponential_pr_open` f510, PR #214 flash f528, exit strip + green `0` badge f545.
Board row gets the green PR dot f530. Effective mono: 13px × 1.8 = **23.4px**.

**Phone PiP at 1.24× the film's size** (330×680 chassis → ~409×843 on screen, right side):
slides in f420 (SETTLE spring, 2°→0°), catch-up cascade of the first 6 feed items
(f424–449, 5f apart), then live mirror: push f456, pr_open f467, done-narration f502.
Take-control pulse f470, input glow on the dock prompt f445. Exits f540–552.

### U3 · Release waves (f558–750, 6.4s) — camera 1.26×

**Hard cut** (no card between U2 and U3 — review r2): the cut lands on the releases
LIST (v0.12 row, `3 of 8 done`) with the orchestrator dock tab already popped and the
create-branch line printing (f562/566) so nothing is dead; drill f592, graph draws from
f602 under the `Or ship a whole release.` caption. Camera `{1.26, 762, 480→472}` — both window edges pinned
(x = 960/s), vertical-only drift: the release sidebar (header + **progress bar**, the
payoff element) at left, the full graph including the `PR #219` chip (fixed at wl
≈1338–1509) at right, and the dock's multiplying tab strip along the bottom; the dock
grid is texture (progress bar + graph + Shipped pill carry the story). Still above the
ships film's 1.15× for this view. Rail: Releases active.
Sidebar: release detail (drill slides f592, finishing as the card lifts) with progress
bar `3 of 8`. Center: flow graph. Dock: orchestrator tab + wave tabs. Compressed
choreography:

| frames | graph | dock | progress |
|---|---|---|---|
| f602–626 | main + `exp/rel-v0-12` lanes draw | create branch f594 · planning f606 | 3 of 8 |
| f610–618 | wave-1 lanes fork | tabs EXP-139/141/143 pop f612/615/618 | — |
| f624–644 | wave-1 merges pulse f626/635/644 | Agent-done lines f624/633/642 | 4 → 5 → 6 |
| f650–653 | wave-2 forks | wave-2 prose f648, tabs f652/655 | — |
| f670–679 | wave-2 merges f670/679 | Agent-done f668/677 | 7 of 8 at f672 |
| f686–694 | PR chip `PR #219` f692 | review prose f686 · pr_open f690 · flash f694 | — |
| f712–732 | merged pulse f722 | `Merged PR #219` auto-ship flash f722 | **8 of 8** f712–724 |

Shipped pill f728, done-cascade f732, then f732–750 the pill holds the frame — the word
itself moves to the standalone C3 card (no punch over UI). Hard cut to live C at f798.
Graph labels: 16px mono × 1.26 = **20.2px** (vs 18.4px in the 50s film).

---

## 4. Type system (screen-space)

- **PUNCH over footage** (live bookends ONLY — never over UI): Inter 700, 64–72px,
  centered, letter-spacing −1.5, #FAFAFA, bottom-third (y ≈ 870–880) with the ships
  scrim. Enter scale 1.04→1 + 8f fade, exit 6f.
- **TYPE CARDS** (`src/spot/overlays.tsx`): full-frame `#09090b` cover with its own soft
  indigo radial glow (the global Background is hidden while covered), Inter 700 **~100px**,
  letter-spacing −2, centered mid-frame. Card bg fades 8f in/out; text enters 6f after the
  bg, exits 6f before. The opaque plateau is where camera cuts / shell switches happen.
- **Wordmark chip** bottom-left during the UI act only (f240–790).
- Max 5 words per line. Copy is locked in `SPOT_COPY` (`src/spot/timeline.ts`).

## 5. Audio plan (post-footage; nothing wired until assets exist)

Three mix states, all automated in Remotion:

1. **Loud** (A): clip A native audio at full — chatter, espresso machine, clatter.
2. **Quiet** (B → U3): Seedance is prompted to duck to keystrokes inside clip A, but we own
   the duck: clip-A volume ramps 1 → 0.35 over f150–195 regardless of what the take does.
   Under the UI act, a **keyboard foley loop** (separate asset, `public/footage/keys.mp3`,
   ~-24 LUFS) ticks under the music — "the typing became the agent". Terminal flashes may
   get single soft key ticks; no UI bleeps.
3. **Warm** (C + outro): clip C native audio (cup clink, "thanks") at full, music resolves.

One continuous **music bed** (generated separately — NOT Seedance's per-clip scoring,
which can't stay coherent across generations): minimal, builds from B, peak at
`Shipped.`, resolves warm over C. **Wired (2026-07-13)** in `src/spot/audio.tsx`:
`music.mp3` (29.7s) is anchored at f120 so its measured arrival (~22s in-track) lands
under the standalone `Shipped.` card (C3) and its natural fade dies into the brand card;
`keys.mp3` loops at low volume under the UI act (f228–750). Both files live in `public/footage/`;
if the track is ever swapped, re-measure its loudness curve (ffmpeg ebur128) and re-anchor.

## 6. Sound-off readability

Every beat reads silent: A/B carry big type + obvious visual states (loud café / macro
typing), U1–U3 carry the punch/cards + the same triple-redundant UI motion as the ships
film (typing, merges, progress bar), C is big type + the cup. **PASS by construction** —
the type IS the voiceover.

## 7. Render / iterate

```bash
cd apps/video
bun run studio            # LaunchSpot composition
bun run render:spot       # → out/launch-spot.mp4 (crf 16)
```

Draft renders at `--scale=0.5`, extract beat frames with ffmpeg, compare against
`ref/*.png` (UI) and the take picks (live).

## 8. Open items

- [x] Generate clip A takes → pick → `public/footage/cafe-a.mp4` (2026-07-13: 11s, 960×960)
- [x] Generate clip C takes → `public/footage/cafe-c.mp4` (10s, 960×960 — bright laptop
      screen visible in the cup shot's left edge; regenerate in 16:9 with "screen turned
      away" pressure for the final)
- [x] Flip `ready: true` in the `FOOTAGE` manifest
- [x] Music bed + keyboard foley wired (`src/spot/audio.tsx`)
- [ ] Grade check: footage gets `saturate(.92) contrast(1.04)` + vignette (built in) — verify
      against the take's own look
- [ ] Vertical 1080×1920 cut for Shorts/Reels (stretch — new camera keys, same timeline)
