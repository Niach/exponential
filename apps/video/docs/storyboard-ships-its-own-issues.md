# Storyboard — "Ships Its Own Issues"

Launch video for Exponential. 1920×1080 @ 30fps, **1500 frames (50.0s)**, entirely recreated UI (no screenshots).
Remotion composition id suggestion: `ShipsItsOwnIssues`.

---

## 0. Global rig (read first — every scene assumes this)

### 0.1 Stage & camera convention

- **Canvas**: the existing `Background` component — `#09090b` fill + two drifting indigo radial glows
  (`rgba(99,102,241,0.20)` @ 50%+drift/10% 32%, `rgba(129,140,248,0.10)` @ 88% 92%). Glow drift runs the
  full 1500 frames (0→40 linear).
- **Desktop window**: recreated gpui IDE staged at **1568×980 native px**, positioned at comp `(176, 50)`,
  radius 10, 1px `rgba(255,255,255,0.10)` border, shadow `0 40px 120px rgba(0,0,0,0.6)`. All desktop-ide.md
  metrics used verbatim (38px top bar, 44px rail, 260px sidebar, 288px props panel, 28px rows, 240px dock).
  Window-local (wl) regions: top bar y 0–38 · rail x 0–44 · sidebar x 44–304 · center x 304–1568 ·
  dock expanded y 740–980 (tab strip 740–769, grid 769–952, exit strip 952–980) · dock collapsed strip y 951–980.
- **Camera**: one `<Camera>` wrapper around the window layer. Keyframes are `{frame, scale s, focus (x,y) wl}`;
  implementation: `translateX = 960 − s·(x+176)`, `translateY = 540 − s·(y+50)`, transform-origin `0 0`.
  Everything camera-related uses **EASE = cubic-bezier(0.16,1,0.3,1)** unless a keyframe says `linear` or `spring`.
- **Screen-space layer** (NOT affected by camera): text overlays, lower-third scrim, wordmark chip, phone PiP.

### 0.2 Overlay system (screen-space)

- Reserved **lower-third safe area**: y 830–1020, left-aligned at x 120 (hook + "Shipped." are centered).
- Scrim behind every overlay: bottom-anchored gradient `rgba(0,0,0,0.55) → transparent`, 220px tall, fades
  in/out with its overlay (6f).
- Styles:
  - **PUNCH** (S1, S11): Inter 64px / 650 (S11: 72px / 700), letter-spacing −1.5, `#FAFAFA`, centered.
  - **CAPTION** (all others): Inter 44px / 600 (S2 uses 40px / 600), letter-spacing −0.5, `#FAFAFA`.
  - Enter: scale 1.04→1.0 + 8f fade (EASE). Exit: 6f fade. Max 5 words per line, max 2 lines.
- **Wordmark chip** (from S2 frame 130 until S11 dim): screen-space bottom-left (120, 990) — real cut-curve
  logo 20px (white disc, stroke-6 curves) + "Exponential" Inter 15px/600, in a `#171717` pill @ 80% opacity,
  1px border white@10.

### 0.3 Fonts & determinism

- Inter 400/500/600/700 + JetBrains Mono 400/700 via `@remotion/google-fonts`. No other faces.
- **Every animation derives from `useCurrentFrame()`** — typing = `text.slice(0, floor((frame−start)·charsPerFrame))`,
  cursor blink = `frame % 16 < 8` (≈533ms cycle), springs via Remotion `spring()`. Zero timers (render-jitter rule).
- Named springs: `POP` = {damping 12, stiffness 200} (pills, badges, tabs), `SETTLE` = {damping 16, stiffness 140}
  (dock resize, dialog).
- Typing speeds: terminal `$` command lines **0.5 frames/char (2 chars/frame ≈ 17ms/char)**; `✓`/`●` result lines
  never type — they fade in whole over 3f with a 4px rise. Line gaps are per-scene (specified below).
- Cursor: custom white SVG pointer with 1px dark outline, screen drop-shadow; moves use EASE; click =
  12px ripple ring expanding over 8f @ 40%→0 opacity + pointer scale 0.9→1 over 4f.

### 0.4 Judge notes addressed (deltas vs. the winning concept)

1. **"Sync/everywhere story untold" (all 3 judges)** → new **phone PiP moment inside S6** (frames 618–702):
   the marketing mobile recreation's live-steer screen slides in screen-space and mirrors the dock's terminal
   lines live (each line lands on the phone 2f after the desktop — the Electric-sync thesis made visible),
   with `● Live` pill, presence `👁 2`, and the green **Take control** button. Second overlay
   "Steer it from your phone." Costs zero extra beats; funded by trimming the diff beat (designated cut) 135f→105f.
2. **"Terminal openers are common — cap on thumbstop" (judge 2)** → hook is layered beyond typing: frame 0 already
   shows the `claude · EXP-142` dock tab + an issue identifier in the stream (product-anchored, not generic),
   a diffstat counter is ticking at f0, a green `✓` result line lands with a flash at f26, and the punch overlay
   snaps at f24 — three distinct motion systems in the first second.
3. **Accuracy fixes against the real desktop spec**: (a) the **issue** Start-coding dialog has NO ultracode switch
   (that's release-variant only) — S5 now ticks **Plan mode** instead and the overlay becomes
   "Model. Effort. Plan mode."; ultracode appears where it really lives, in the S9 release dialog
   ("Dynamic workflows (ultracode)", default ON). (b) "Fable" verified: the real picker is Fable/Opus/Sonnet,
   default Fable (desktop-ide.md §12.1). (c) S7 Changes header reads `PR #214` (session already ended), not
   "Claude running".
4. **Pacing (tight 12 beats)** → frame-exact budget below, per-scene readability floors (§5), and designated
   trims (§6) that keep the hook, dialogs, and waves untouched.
5. **Legibility at feed size** → every information beat runs at ≥1.35× camera (≥18px effective primary text);
   sub-threshold text is explicitly declared "texture" and never carries story (§5 table).

---

## 1. Scene table (30fps · 1500 frames total)

| # | Scene | from | durationInFrames | ends | Transition IN | Transition OUT |
|---|-------|------|------------------|------|---------------|----------------|
| S1 | HOOK — terminal macro | 0 | 75 | 75 | none (cold open, already mid-motion) | continuous camera (no cut) |
| S2 | REVEAL — full shell pull-back | 75 | 105 | 180 | continuous camera | **hard cut** (the only act-one cut; also resets the flash-forward) |
| S3 | BOARD — pick an issue | 180 | 120 | 300 | hard cut + row cascade | continuous (detail slides over) |
| S4 | DETAIL — issue page | 300 | 105 | 405 | slide-in from right 46px | continuous (dialog opens over) |
| S5 | DIALOG — start coding | 405 | 120 | 525 | dialog scale 0.96→1 + scrim | dialog collapses down toward dock |
| S6 | DOCK RUN — Claude session + phone PiP | 525 | 180 | 705 | dock springs 29→240px, camera tilts down | continuous (segmented-pill morph) |
| S7 | DIFF — Changes tab | 705 | 105 | 810 | Details→Changes pill morph 8f | continuous pan-left |
| S8 | MERGE — Reviews | 810 | 120 | 930 | camera pan to rail 12f | **whip-pan 6f** |
| S9 | RELEASE — panel + release dialog | 930 | 150 | 1080 | whip-pan 6f | dialog collapse + center crossfade 8f |
| S10 | WAVES — branch flow graph | 1080 | 180 | 1260 | center crossfade to graph | continuous |
| S11 | SHIPPED — progress complete | 1260 | 120 | 1380 | continuous (final merge lands here) | shell dims 20% → **hard cut** |
| S12 | OUTRO — brand card | 1380 | 120 | 1500 | hard cut to canvas | end (hold, 8f fade on last frames) |

Beat-to-timecode: S1 0:00–2.5 · S2 2.5–6.0 · S3 6.0–10.0 · S4 10.0–13.5 · S5 13.5–17.5 · S6 17.5–23.5 ·
S7 23.5–27.0 · S8 27.0–31.0 · S9 31.0–36.0 · S10 36.0–42.0 · S11 42.0–46.0 · S12 46.0–50.0.

---

## 2. Shared fixture dataset (the ONE world every scene draws from)

### 2.1 Identity & repo

- Workspace **Exponential** · project **Exponential** (type dev, project color indigo `#6366F1`, code glyph),
  prefix **EXP**. Repo **niach/exponential**, default branch `main`.
- User: **Alex Carter** — avatar initials **AC** (#262626 circle, 12px/500), device **MacBook Pro**.
- Top bar state (whenever visible): project pill `‹code glyph #6366F1› Exponential ⌵` · run select
  **Dev Server** + green ▶ · divider · git cluster `⎇ main` · ✓ commit icon · context chip `↑1`.
- Dock tabs across the video: `claude · EXP-142`, `zsh`, later `claude · release v0.12`,
  `claude · EXP-139`, `claude · EXP-141`, `claude · EXP-143`, `claude · EXP-144`, `claude · EXP-145`.

### 2.2 Issue board (All Issues, project Exponential)

| Group (tint) | Row |
|---|---|
| In Progress `rgba(234,179,8,.10)` (1) | `=` EXP-139 · ⏱ · "Release progress bar drifts when issues are cancelled" · ‹bug› · DS · 📅 Jul 14 |
| Todo `rgba(212,212,216,.08)` (3) | `↑` **EXP-142** · ○ · "Live-steer terminal reconnect" · ‹bug› · DS · 📅 Jul 15 |
| | `=` EXP-143 · ○ · "Confirm-merge double-fires on slow connections" · ‹desktop› · — · 📅 |
| | `↓` EXP-144 · ○ · "Branch flow graph clips long release names" · ‹desktop› · DS · 📅 |
| Backlog `rgba(113,113,122,.08)` (2) | `–` EXP-146 · ◌ · "Offline board cache for the web app" · ‹web› · — |
| | `–` EXP-147 · ◌ · "Widget screenshot annotations on Safari" · ‹widget› · — |
| Done `rgba(34,197,94,.10)` (2) | `=` EXP-138 · ✓ · "Exit-code badges on orchestrator tabs" · DS |
| | `↓` EXP-140 · ✓ · "Ship desktop auto-update banner" · DS |

Priority glyph key: `↑`=SignalHigh #F97316, `=`=SignalMedium #FACC15, `↓`=SignalLow #3B82F6, `–`=Minus muted.
Labels: bug (red dot), desktop (blue dot), web (indigo dot), widget (green dot) — rounded-full chips,
6px dot + 12px muted text.

**State timeline**: EXP-142 sits in Todo until the S5 launch click (f516) → flips to In Progress (⏱ #FACC15);
gets a 6px green "PR open" dot at f696; flips to Done ✓ #22C55E and slides into the Done group at f904–922.

### 2.3 Hero issue EXP-142

- Title: **Live-steer terminal reconnect**
- Description (GFM, 2 paragraphs): "When the steer relay drops a WebSocket mid-session, the terminal view goes
  stale and never recovers. Reconnect with exponential backoff and resume the scrollback buffer." /
  "Repro: restart the relay while a session is streaming — the viewer freezes until a full reload."
- Properties: STATUS ○ Todo → ⏱ In Progress · PRIORITY ↑ High · ASSIGNEE AC Alex Carter ·
  LABELS ‹● bug› · RELEASE 🚀 v0.12 · DUE DATE 📅 Jul 15 · PROJECT ‹● Exponential›.
- Activity: "Activity (2)" — "**Alex** added label bug" · "**Alex** added this to release v0.12".
- Branch `exp/EXP-142` · worktree `.worktrees/EXP-142` · **PR #214** ("Live-steer terminal reconnect").

### 2.4 Hero Claude session script (dock tab `claude · EXP-142`) — canonical 10 lines

Line grammar: `$ ` muted #A1A1A1 prompt + #FAFAFA command · `✓ ` #22C55E · `● ` #F97316 (Claude bullet) ·
output #A1A1A1. JetBrains Mono 13px/1.3.

```
 1  ✓ Created worktree .worktrees/EXP-142 on branch exp/EXP-142
 2  ✓ Handing EXP-142 to Claude
 3  ● Reading issue EXP-142 — Live-steer terminal reconnect
 4  ● Plan: reconnect with exponential backoff, resume scrollback
 5  ● Edited apps/web/src/components/agent-session.tsx (+38 −11)
 6  $ bun run typecheck
 7  ✓ typecheck passed · 0 errors
 8  $ git push -u origin exp/EXP-142
 9  ● Opening PR via exponential_pr_open
10  ✓ Opened PR #214 — Live-steer terminal reconnect
    ── exit strip: ● Process finished with exit code 0  (green dot, tab badge `0` green@15)
```

S1/S2 show lines 3–9 of this same session as a **flash-forward** (cold open); S6 plays the full script.
The S2→S3 hard cut is the flash-forward reset (dock collapses to "Terminal (1)", story restarts at "Pick an issue").

### 2.5 EXP-142 diff (Changes tab)

Header: `⎇ exp/EXP-142 · PR #214 · 5 files +120 −34` · right: ghost "Open terminal in worktree" + `…`.
File list (240px, mono 12px status letters): `M apps/web/src/components/agent-session.tsx` ·
`M apps/web/src/lib/steer.ts` · `A apps/web/src/lib/steer-backoff.ts` · `M apps/web/src/hooks/use-agent-stream.ts` ·
`A apps/web/src/lib/steer-backoff.test.ts`.
Hero hunk (agent-session.tsx, side-by-side, 11px mono, 18px rows):

```
@@ -48,11 +48,29 @@ export function AgentSessionView({ sessionId }: AgentSessionProps)
-  const socket = connect(relayUrl)
+  const socket = connectWithBackoff(relayUrl, {
+    base: 500,
+    factor: 2,
+    maxDelay: 15_000,
+    onResume: (buffered) => terminal.write(buffered),
+  })
```

plus ~14 context rows above/below (syntax tinting: keywords #60A5FA, strings #4ADE80, numbers #FACC15).

### 2.6 Release v0.12

- Releases panel row: `🚀 v0.12` · sub-line `Target Jul 18 · 3 of 8 done`.
- 8 member issues: done at S9 = EXP-138, EXP-140, **EXP-142** (act one feeds the release — continuity beat);
  open = EXP-139, EXP-141 ("Public board hides pr_url for anonymous viewers"), EXP-143, EXP-144,
  EXP-145 ("Orchestrator wave logs collapse into one tab").
- Release dialog (560px, title **"Start coding on release"**): intro `One Claude orchestrator implements the
  checked issues of "v0.12" — one subagent per issue.` · checklist grouped under repo header **niach/exponential**,
  5 checked rows (mono id + title) · **Model: Fable** · **Effort: Max** · **Subagent model: Inherit** ·
  **Subagent effort: High** · switch row **"Dynamic workflows (ultracode)"** ON (indigo) with caption
  `Runs the orchestrator with --effort ultracode — works with any model.` · "Plan mode" unchecked ·
  footer primary **Start coding**.
- Integration branch `exp/rel-v0-12`. Wave 1 = EXP-139/141/143, wave 2 = EXP-144/145. Release **PR #219**.
- Orchestrator script (dock tab `claude · release v0.12`):

```
✓ Created integration branch exp/rel-v0-12
● Planning waves: 2 waves · 5 issues
● Wave 1: EXP-139 · EXP-141 · EXP-143 — 3 subagents
✓ Merged exp/EXP-139 into exp/rel-v0-12
✓ Merged exp/EXP-141 into exp/rel-v0-12
✓ Merged exp/EXP-143 into exp/rel-v0-12
● Wave 2: EXP-144 · EXP-145
✓ Merged exp/EXP-144 into exp/rel-v0-12
✓ Merged exp/EXP-145 into exp/rel-v0-12
● Reviewing combined diff — 23 files +612 −188
✓ Opened PR #219 — Release v0.12
```

### 2.7 Branch flow graph (center-pane hero view, S10)

Lanes (34px rows, mono **16px** labels — drawn large because this view is net-new):
`main ✓` → `● exp/rel-v0-12` → forks `exp/EXP-139`, `exp/EXP-141`, `exp/EXP-143` (wave 1), then
`exp/EXP-144`, `exp/EXP-145` (wave 2). Rails 1px `muted @ 35%`; PR-state dots 8px (yellow #FACC15 open-work →
green #22C55E merged); trailing muted `↑2` / `↑1 ↓1` counts; merge points pulse green (12px ring, 8f).

### 2.8 Phone PiP fixture (mobile steer screen, from the marketing mob-* recreation)

330px-wide iPhone frame (#050505, 44px radius, dynamic island, 20:22 status bar). Steer screen: 32px round
back button · `EXP-142` mono · `● Live` pill (`rgba(52,211,153,.15)` bg, #34D399 text) · presence chip `👁 2`.
Terminal card (radius 14, #050505, JetBrains Mono 11px/1.55) **mirrors dock lines 3–10 live, each 2f after
the desktop**. Bottom: key row `esc ^C tab ↑ ↓`, input "Type to steer…", green **Take control** button
(#34D399 bg, #052E1C text).

### 2.9 Outro card

Real cut-curve logo (white disc r=50, three stroke-6 bezier cut curves — exact paths from video-brand.md §3),
wordmark "Exponential" Inter 96px/700 ls −3, tagline **"Issue tracking that ships code."** Inter 36px/500
#A1A1A1, URL **exponential.at** JetBrains Mono 28px #A1A1A1 + blinking #FAFAFA block cursor.

---

## 3. Scenes

### S1 · HOOK — terminal macro (f0–75, 2.5s)

**Camera**: f0 `{scale 1.8, focus wl (600, 855)}` → f75 `{1.8, (640, 855)}` — a 40px rightward drift, `linear`
(constant motion under the punch line). The visible crop: dock tab strip with **`claude · EXP-142`** active tab
(top edge of frame) + terminal grid. Nothing else of the app is identifiable — near-black #0A0A0A monospace world.

**UI state**: dock expanded (240px), tab `claude · EXP-142` active + a dimmed `zsh` tab. Grid already holds
script lines 3–5 (§2.4). Block cursor `#FAFAFA` blinking at `frame % 16 < 8` on the input row.

**Micro-animations** (motion from frame 0 — no fade-in anywhere):
- f0–20: line 5's diffstat **counts up** `(+12 −3) → (+38 −11)` (rolling mono digits, linear) — non-typing
  motion at frame zero.
- f8: `$ bun run typecheck` begins typing at 2 chars/frame (17 chars → done f17).
- f26: `✓ typecheck passed · 0 errors` lands — whole-line 3f fade + 4px rise, and the ✓ glyph flashes
  #22C55E @ 100%→70% over 6f.
- f40: `$ git push -u origin exp/EXP-142` begins typing (31 chars → done f56); its muted output line
  `→ remote: resolving deltas… done` fades in f64.

**Overlay** (PUNCH, centered, two lines, enters f24: scale 1.04→1.0 + 8f fade; exits with S2's overlay swap f92):
> **Your issue tracker.**
> **Writing code.**
Inter 64px/650, ls −1.5, #FAFAFA, centered at (960, 880/952), scrim on.

**Build note**: ports the marketing `Terminal.tsx` grid + line grammar 1:1; only new work is the frame-driven
script clock and the digit-roll diffstat. **Complexity: LOW.** Hard part: keeping the crop composition clean at
1.8× (tab strip must sit just inside the top edge).

---

### S2 · REVEAL — full shell pull-back (f75–180, 3.5s)

**Camera**: f75 `{1.8, (640, 855)}` → f120 `{1.0, (784, 490)}`, EASE (45f) → hold to f180. The window is revealed
floating on the glow canvas.

**UI state** (flash-forward continues): full shell — top bar (§2.1 state), rail with **All Issues** active
(list-todo icon tinted #6366F1 + 2px accent bar), sidebar = All Issues board (§2.2 with EXP-142 already ⏱
In Progress — it's the future), center tab `EXP-142` showing the issue detail, dock still expanded and streaming:
f130 `● Opening PR via exponential_pr_open` fades in; f156 `✓ Opened PR #214 — Live-steer terminal reconnect`.

**Micro-animations**:
- f105: green **Coding now pill** springs into the detail header (POP scale 0→1): rounded-full,
  1px `#22C55E @ 40%` border, 6px green dot, "Coding now · MacBook Pro" 12px.
- f130: wordmark chip (§0.2) fades in bottom-left over 10f — persists until S11.
- Dock lines keep their 3f fade-in rhythm underneath.

**Overlay** (CAPTION 40px/600, lower-third left): `Exponential — desktop IDE` — in f96, out f168.

**Build note**: full shell assembly — ports marketing `Topbar/Rail/Sidebar/Board/IssueDetail/Terminal` restaged
from 960×640 to the 1568×980 metric grid (absolute layout re-flow, not a redesign). **Complexity: MEDIUM**
(the restage is the cost; every subsequent scene reuses it).

---

### S3 · BOARD — pick an issue (f180–300, 4s)

**Transition in**: **hard cut** at f180 (the one act-one cut). Simultaneous state reset: dock collapsed to the
29px strip (`▣ Terminal (1)` + chevron-up), center pane = empty state (`inbox` icon 24px muted, **"Nothing open"**
13px/500, "Pick an issue from the sidebar — it opens as a tab here." 12px muted), EXP-142 back to ○ Todo,
no Coding-now pill. (Flash-forward over; the empty-state copy rhymes with the overlay — deliberate.)

**Camera**: fixed `{1.55, (352, 430)}` — sidebar board fills the left 40%, empty-state center visible right.

**UI state**: All Issues tool — title row "All Issues" + list-filter ghost + indigo **New Issue** button;
pill tabs `All Issues`(active #262626) · `Active` · `Backlog`; groups per §2.2.

**Micro-animations**:
- f180–213: rows cascade-enter top-to-bottom — each row fades + rises 12px over 9f, **3f stagger** (8 rows +
  4 group headers, headers lead their group).
- f216–240: cursor glides in from right edge to the EXP-142 row (24f, EASE).
- f240: row hover — bg `accent @ 30%`, hover checkbox ghosts in at 40%.
- f276: **click** — ripple ring (§0.3), row flashes selected (solid #262626) for 6f.

**Overlay** (CAPTION): `Pick an issue.` — in f192, out f288.

**Build note**: direct port of marketing `Board.tsx` with new fixture rows; cascade + cursor rig are shared
utilities. **Complexity: LOW.**

---

### S4 · DETAIL — issue page (f300–405, 3.5s)

**Transition in**: center pane — tab `EXP-142` pops into the tab strip (POP), detail **slides in from the right
46px + fade over 20f** (f300–320).

**Camera**: f300 `{1.55, (352, 430)}` → f330 `{1.4, (800, 330)}`, EASE — settles on the header row + title,
properties panel visible on the right.

**UI state** (§2.3): header row `Details · Changes` segments left; right cluster `3 / 8 ˄ ˅` switcher ·
green ▶ **Start coding** · 🔔 Subscribe. Title "Live-steer terminal reconnect" 20px/600; description (2 GFM
paragraphs); full-bleed divider; "Activity (2)" rows; "Leave a reply…" composer. Properties panel: 11px UPPERCASE
labels STATUS/PRIORITY/ASSIGNEE/LABELS/RELEASE/DUE DATE/PROJECT with values per §2.3 — note **RELEASE 🚀 v0.12**
(plants the act-two payoff).

**Micro-animations**:
- f306–334: properties labels + values stagger-fade top-to-bottom (4f stagger, 8px rise each).
- f350–374: cursor moves to **▶ Start coding** (24f, EASE).
- f374: button hover — bg `accent`, play glyph brightens to full #22C55E.
- f390: **click** (ripple).

**Overlay**: none (concept-intentional breather). Sound-off story carried by the giant green Start-coding
affordance + cursor.

**Build note**: ports marketing `IssueDetail.tsx`; adds the RELEASE property row (new but trivial).
**Complexity: LOW-MEDIUM** (markdown body is static JSX, no editor logic).

---

### S5 · DIALOG — start coding (f405–525, 4s)

**Transition in**: black@50 scrim fades 8f; dialog (420px, radius 8, #171717, title **"Start coding on EXP-142"**)
scales 0.96→1.0 + fade over 10f (SETTLE).

**Camera**: f405 `{1.4, (800, 330)}` → f420 `{1.45, (784, 470)}`, EASE — dialog centered, fills ~40% of frame
(effective dialog width ≈ 609px on screen).

**UI state** (desktop-ide.md §12.1 verbatim): intro `Claude works on EXP-142 in its own worktree and opens the
pull request when done.` (12px muted); side-by-side labeled selects **Model** / **Effort**; **Plan mode**
checkbox row; footer primary **Start coding**.

**Micro-animations** (rapid but readable):
- f420: Model select opens — menu pops 6f, options `Fable ✓` / `Opus` / `Sonnet`.
- f432: cursor confirms **Fable** (check pulses); menu closes f438. (18f total ≈ 600ms.)
- f447: Effort select opens — `CLI default / Low / Medium / High / XHigh / Max`; cursor flicks to **Max** f456;
  closes f462 (≈500ms).
- f472: **Plan mode** checkbox ticks ON — 6f check-draw + subtle POP on the box.
- f500: cursor to the primary button; hover fill brightens.
- f516: **click** → button label swaps to `Starting…` + 12px spinner for 6f.
- f516–525: dialog **collapses downward toward the dock** — scale 1→0.9, translateY +80px, fade, EASE
  (motion vector hands off to S6's dock expansion).
- Board row EXP-142 flips ○→⏱ (#FACC15) at f519 behind the scrim (visible as the scrim lifts).

**Overlay** (CAPTION): `Model. Effort. Plan mode.` — in f417, out f510.

**Build note**: net-new small surface (issue dialog) — plain stack of shadcn-style rows, two fake selects, one
checkbox. **Complexity: LOW-MEDIUM.** Hard part: choreographing three input interactions to read at speed
(each interaction holds its end-state ≥12f).

---

### S6 · DOCK RUN — Claude session + phone PiP (f525–705, 6s)

**Camera**: f525 `{1.45, (784, 470)}` → f543 `{1.5, (700, 840)}`, EASE — tilts down onto the dock. Hold; then
f688–705 drift focus x 700→760 (linear) under the PR landing.

**UI state**: dock springs **29→240px over 18f (SETTLE)** starting f525; tab `claude · EXP-142` pops (POP) at
f531 next to `zsh`; strip label was "Terminal (1)" → now 2 tabs. Sidebar board remains visible above-left
(EXP-142 ⏱ In Progress). Grid empty at f543, then the full §2.4 script plays:

| frame | line | motion |
|---|---|---|
| f546 | 1 ✓ Created worktree… | 3f fade + rise |
| f566 | 2 ✓ Handing EXP-142 to Claude | gap 20f |
| f584 | 3 ● Reading issue EXP-142 — Live-steer terminal reconnect | gap 18f |
| f600 | 4 ● Plan: reconnect with exponential backoff, resume scrollback | gap 16f |
| f615 | 5 ● Edited apps/web/src/components/agent-session.tsx (+38 −11) | gap 15f |
| f630 | 6 $ bun run typecheck | **types 2 chars/frame** (17 chars) |
| f648 | 7 ✓ typecheck passed · 0 errors | gap 12f |
| f660 | 8 $ git push -u origin exp/EXP-142 | types 2 chars/frame (31 chars) |
| f681 | 9 ● Opening PR via exponential_pr_open | gap 10f |
| f693 | 10 ✓ Opened PR #214 — Live-steer terminal reconnect | lands with green flash |

Gaps accelerate 20f→10f (the "it's alive" ramp). Block cursor blinks on the active row throughout.

**Phone PiP (screen-space)**: f618–633 iPhone frame (§2.8) slides in from x 2020 → rests at (1445, 165),
spring (SETTLE) + 2° → 0° rotation. Its terminal card mirrors dock lines 3→10, **each appearing 2f after the
desktop line** (the sync beat). `● Live` pill pulses (opacity 0.7↔1, 24f cycle); `👁 2` presence chip;
green **Take control** button gets one soft glow pulse at f660. Exits f690–702 (slide right + fade).

**Cross-surface sync moment**: f696 — a 6px **green PR dot** pops (POP) onto the EXP-142 board row in the
sidebar, simultaneous with the phone's mirrored line-10 landing.

**End state**: f699 exit strip fades in — `● Process finished with exit code 0` (6px green dot, 12px muted);
tab gains the green `0` exit badge (POP).

**Overlays** (CAPTION): `Claude runs in the dock.` in f537, out f612 → `Steer it from your phone.` in f622,
out f698.

**Build note**: dock/terminal port from marketing (script clock reused from S1); PiP ports the marketing
mobile steer screen (`mob-*`) as a screen-space component with a line-mirror offset. **Complexity: MEDIUM**
(two synced script clocks + PiP restage). Hard part: layout — PiP must never cover the dock's typing column
(dock content is left-biased at this focus; PiP owns the right 25%).

---

### S7 · DIFF — Changes tab (f705–810, 3.5s)

**Transition in**: segmented control `Details → Changes` — the active pill **morphs across** (position+width
interpolate, 8f EASE) at f705–713; center content crossfades 6f.

**Camera**: f705 `{1.5, (700, 840)}` → f725 `{1.5, (930, 400)}`, EASE — up onto the diff. Then a slow
**vertical scroll**: focus y 400→460, f740–800, linear (the "reading" move).

**UI state** (§2.5): header `⎇ exp/EXP-142 · PR #214 · 5 files +120 −34` (branch mono #FAFAFA, +120 #22C55E,
−34 #EF4444); 240px file list left (5 rows, status letters M/A tinted); side-by-side diff right — file header bar,
blue-tinted `@@ -48,11 +48,29 @@` hunk header, then the §2.5 hunk with syntax tinting.

**Micro-animations**:
- f716–748: diff rows **paint in top-to-bottom, 1 row per frame**; each added row flashes
  `#22C55E @ 20%` → settles to 10% over 8f (removed rows same in red).
- f752: file-list row `agent-session.tsx` gets the selected tint (auto-follow of the scroll).
- Stats `+120 −34` in the header digit-roll from 0 during f713–725.

**Overlay** (CAPTION): `Review it in place.` — in f717, out f798.

**Build note**: ports marketing `Diff.tsx` (side-by-side, syntax regex tinting) with new fixture content.
**Complexity: LOW.** This is also **designated trim #1** (can drop to 75f by cutting the scroll).

---

### S8 · MERGE — Reviews (f810–930, 4s)

**Camera**: f810 `{1.5, (930, 430)}` → f822 `{1.5, (330, 430)}`, EASE 12f — pan left to rail + sidebar.

**UI state**: rail — **Reviews** (git-pull-request) becomes active at f822: icon tints #6366F1, 2px accent bar
slides in (6f), its amber notification dot visible before the click, gone after. Sidebar crossfades (6f) to the
Reviews tool: group header `● Exponential` (8px indigo dot, 12px semibold muted), one row:
```
⑂ EXP-142  Live-steer terminal reconnect        [Merge]
   #214 · exp/EXP-142
```
(⑂ icon #22C55E, outline xsmall Merge button.)

**Micro-animations**:
- f834–852: cursor to **Merge**; hover.
- f855: click → button **morphs to red-outlined "Confirm merge"** (width animates, 6f; danger #FF6467 border/text).
- f882: second click → `Merging…` + spinner for 8f.
- f890–902: row fades out + collapses (height→0, EASE).
- f902–908: sidebar crossfades back to the **All Issues board**.
- f904–922: EXP-142's status glyph flips ⏱→**✓ filled #22C55E** (POP) and the row **slides down into the
  green-tinted Done group** (translateY through the list, 18f EASE; Done count ticks 2→3).

**Overlay** (CAPTION): `Merge. Done.` — in f822, out f918.

**Build note**: Reviews panel is a light net-new build (two-line row + two-state button, per spec §5.3);
the board row re-group animation is the tricky bit (FLIP-style position tween between two list layouts).
**Complexity: MEDIUM.**

---

### S9 · RELEASE — panel + release dialog (f930–1080, 5s)

**Transition in**: **whip-pan 6f** (f930–936): camera translateX overshoots 60px with 3px directional blur,
lands `{1.5, (352, 430)}`. Rail: **Releases** (rocket) goes active (accent bar slide).

**UI state part 1** (f936–984): sidebar Releases tool — header `🚀 Releases  +`; row per §2.6
(`🚀 v0.12` / `Target Jul 18 · 3 of 8 done`). f956: cursor clicks the row → **detail drill-in slides left 24px**
(12f): back chevron + `v0.12` header, action row `+ Add issues · ▶ Start coding · …`, summary block with
meta chips (`📅 Target Jul 18`) and the **4px progress bar** — green fill at 3/8 (37.5%), right label
`3 of 8 done` — below it the release's issues grouped by status (3 in Done incl. EXP-142 ✓).

**UI state part 2** (f984–1080): f984 cursor clicks **▶ Start coding** → camera eases to `{1.35, (784, 470)}`
(10f) as the **release dialog** (§2.6, 560px) scales in (SETTLE, 10f) over the black@50 scrim.

**Micro-animations**:
- f1002–1010: checklist rows tick-cascade — the 5 checked rows' checkmarks draw in with 2f stagger
  (they arrive pre-checked; the cascade is a shimmer highlight sweeping down, not state changes).
- f1014–1038: the four selects populate with a fast value-flick cascade, 6f stagger:
  Model→`Fable`, Effort→`Max`, Subagent model→`Inherit`, Subagent effort→`High`.
- f1040: **"Dynamic workflows (ultracode)"** switch — already ON — gets a single indigo thumb-pulse
  (scale 1→1.15→1, 8f) + its caption line brightens 60%→100% for 12f (draws the eye without faking a toggle).
- f1068: **Start coding** click → `Starting…` spinner; an **indigo pulse** (#4F46E5 @ 25% wash) travels down
  the dialog over 12f (f1068–1080).

**Overlay** (CAPTION): `Or ship a whole release.` — in f942, out f1074.

**Build note**: **net-new budget item #1** — Releases panel + detail (progress bar, chips) and the 560px release
dialog (checklist group, 4 selects, switch). All static-layout; no logic. **Complexity: MEDIUM-HIGH — build first**
(fallback per concept: if the S10 graph slips, this dialog + progress fill still lands the act-two story).

---

### S10 · WAVES — branch flow graph (f1080–1260, 6s)

**Transition in**: dialog collapses toward the dock (as S5, 8f); center pane crossfades (8f) to the **branch
flow graph** view; dock springs open again (18f) with new tab `claude · release v0.12` (POP, f1086); sidebar
keeps the release detail (progress bar visible the whole scene).

**Camera**: f1080 `{1.35, (900, 420)}` → f1110 `{1.15, (830, 500)}`, EASE — wide enough to hold graph (center),
progress bar (sidebar) and dock tabs (bottom) at once. Micro-push f1190–1210 `{1.15→1.22, (860, 480)}` on wave 2,
then hold.

**Choreography** (graph §2.7 + orchestrator script §2.6 + progress bar, all frame-locked):

| frames | graph | dock | sidebar progress |
|---|---|---|---|
| f1092–1116 | `main` + `exp/rel-v0-12` lanes draw left→right (stroke-dashoffset, 24f, EASE) | f1090 `✓ Created integration branch exp/rel-v0-12` · f1104 `● Planning waves: 2 waves · 5 issues` | `3 of 8 done` |
| f1120–1140 | **wave 1**: 3 lanes `exp/EXP-139/141/143` fork simultaneously (16f draws) | f1116 `● Wave 1: …` · tabs `claude · EXP-139/141/143` POP with 3f stagger f1122–1128 | — |
| f1150–1184 | PR dots flip yellow→green staggered (f1150/1162/1174); lanes **merge back** into `exp/rel-v0-12` with green pulse rings (f1158/1170/1182) | `✓ Merged exp/EXP-139…` f1160 · `…141…` f1172 · `…143…` f1184 | bar ticks **4 → 5 → 6 of 8** (each: 6f fill segment, EASE + label swap) |
| f1190–1206 | **wave 2**: 2 lanes `exp/EXP-144/145` fork; tabs POP f1194/1197 | f1190 `● Wave 2: EXP-144 · EXP-145` | — |
| f1216–1240 | dots green f1216/f1228; merge pulses f1222/f1234 | `✓ Merged exp/EXP-144` f1224 · `✓ Merged exp/EXP-145` f1236 | **7 of 8** at f1226 (8th lands in S11) |
| f1244–1258 | `exp/rel-v0-12` lane brightens; a PR chip `PR #219 · open` pops onto it (POP) | f1244 `● Reviewing combined diff — 23 files +612 −188` · f1254 `✓ Opened PR #219 — Release v0.12` + green flash | — |

**Overlay** (CAPTION): `One orchestrator. Waves of agents.` — in f1092, out f1250.

**Build note**: **net-new budget item #2 and the single hardest build** — an SVG lane graph with dash-offset
draws, fork/merge geometry, pulse rings, synced to two other surfaces. Draw labels at 16px mono (this view is
ours, not a 1:1 spec port — legibility wins). **Complexity: HIGH — prototype in week 1**; fallback: drop the
graph, run the same choreography on dock tabs + progress bar alone.

---

### S11 · SHIPPED (f1260–1380, 4s)

**Camera**: f1260 `{1.22, (860, 480)}` → f1320 `{1.0, (784, 490)}`, EASE — final pull-back to the full window.

**Choreography**:
- f1262: last merge pulse on the graph (wave-2 tail).
- f1266–1278: progress bar **fills the final segment** (12f, expo-out) → label `8 of 8 done`.
- f1272: orchestrator tab prints `✓ Merged PR #219` (the webhook auto-ship beat, one quiet line).
- f1290: **"Shipped" pill** springs in next to the release name — POP scale 0→1.06→1, rounded-full,
  1px `#22C55E @ 40%` border, green text. Release row's rocket icon tints green.
- f1300: release-detail issue groups: remaining rows cascade into the green Done group (3f staggers) — the
  board equivalent of confetti, using only real UI.
- f1330–1345: entire shell dims to 80% brightness (focus hand-off to the overlay); wordmark chip fades out.

**Overlay** (PUNCH): `Shipped.` — Inter 72px/700, centered (960, 900), in f1300 (scale 1.04→1 + 8f), out f1374.

**Build note**: pure re-use of S9/S10 surfaces + one pill spring. **Complexity: LOW.**

---

### S12 · OUTRO — brand card (f1380–1500, 4s)

**Transition in**: hard cut to the bare canvas (`Background` glows keep drifting — continuity of light).

**Choreography** (all centered column, gap 28):
- f1386–1406: **logo draws** — white disc fades in 8f, then the three cut curves stroke-reveal
  (dash-offset, 20f, 4f stagger between curves, EASE). Logo 118px.
- f1400–1416: wordmark **"Exponential"** (Inter 96px/700, ls −3, #FAFAFA) rises 16px + fades (16f).
- f1408–1424: tagline **"Issue tracking that ships code."** (Inter 36px/500, #A1A1A1) rises 16px + fades,
  6f behind the wordmark.
- f1424–1436: **`exponential.at`** (JetBrains Mono 28px, #A1A1A1) fades in; a #FAFAFA block cursor blinks
  after it (`frame % 16 < 8`) until the end.
- f1492–1500: 8f fade to black.

**Overlay text verbatim** (this scene IS the overlay): `Exponential` · `Issue tracking that ships code.` ·
`exponential.at`.

**Build note**: replaces the placeholder striped-sphere Logo with the real §2.9 SVG (exact paths in
video-brand.md §3, stroke-width 6, unique mask ids). **Complexity: LOW.**

---

## 4. Per-scene build-complexity summary & build order

| Priority | Item | Scenes | Complexity | Source |
|---|---|---|---|---|
| 1 | Branch flow graph center view | S10, S11 | **HIGH** (net-new; prototype first) | net-new SVG |
| 2 | Releases panel + detail + release dialog | S9, S10, S11 | MEDIUM-HIGH (net-new, static) | net-new per desktop spec §5.4/§12.2 |
| 3 | Shell restage 960×640 → 1568×980 | all | MEDIUM (one-time) | marketing `ide-*` port |
| 4 | Camera rig + cursor + overlay/scrim system | all | MEDIUM | net-new utilities |
| 5 | Terminal script clock (frame-driven) + digit rolls | S1, S2, S6, S10 | LOW-MEDIUM | port marketing `codingScriptFor` grammar |
| 6 | Phone PiP steer screen | S6 | LOW-MEDIUM | port marketing `mob-*` steer |
| 7 | Issue Start-coding dialog | S5 | LOW-MEDIUM | net-new per §12.1 |
| 8 | Reviews panel + FLIP row re-group | S8 | MEDIUM | light net-new + Board port |
| 9 | Board / IssueDetail / Diff / dock | S3, S4, S7 | LOW | direct marketing ports |
| 10 | Real logo + outro card | S12 | LOW | video-brand.md §3 |

Fallback ladder (unchanged from concept, now frame-mapped): if the S10 graph slips → run S10 on multiplying
dock tabs + progress ticks only; if schedule slips further → trim S7 to 75f and S2 to 90f (see §6).

---

## 5. Sound-off readability check (every beat must read silent)

| Scene | Silent read | Primary text @ effective size |
|---|---|---|
| S1 | Monospace Claude session typing + "Your issue tracker. Writing code." — parses as "AI agent coding inside a dev tool" with zero audio | mono 13px × 1.8 = **23.4px** ✓; overlay 64px ✓ |
| S2 | Pull-back reveals it's ONE desktop app; caption names it | overlay 40px ✓; UI is context, not info |
| S3 | Overlay "Pick an issue." + cursor + click; center empty-state copy literally repeats the instruction | row title 13 × 1.55 = **20.2px** ✓ |
| S4 | No overlay — story is the giant green ▶ Start coding + cursor path; RELEASE v0.12 chip plants act two | title 20 × 1.4 = 28px ✓; button label 12 × 1.4 = 16.8px ✓ |
| S5 | Overlay names the three controls being touched, in order | dialog text 13 × 1.45 = **18.9px** ✓ |
| S6 | Two overlays split the beat: dock story, then phone story; the mirrored lines make "sync" visible without words | mono 13 × 1.5 = **19.5px** ✓; PiP 11px mono is declared **texture** — its story carriers are the Live pill + Take control button + overlay |
| S7 | Overlay + green/red tint blocks + header `+120 −34` read as "code review" instantly; code content is texture | header mono 12 × 1.5 = **18px** ✓; diff code 11 × 1.5 = 16.5px (texture) |
| S8 | Overlay "Merge. Done." + button morph to red confirm + row physically moving into the green Done group | row text 12–13 × 1.5 = **18–19.5px** ✓ |
| S9 | Overlay + "3 of 8 done" progress bar + a dialog full of checked issues = "batch the rest" | dialog 13 × 1.35 = **17.6px** ✓; progress label reinforced by bar geometry |
| S10 | Overlay + lanes forking/merging + tab strip multiplying + bar ticking — three synchronized motions, no words needed | graph labels 16 × 1.15–1.22 = **18.4–19.5px** ✓; orchestrator terminal = texture |
| S11 | Bar completes → "Shipped" pill → overlay "Shipped." Triple redundancy | pill 12 × 1.0→ small, but overlay 72px carries it ✓ |
| S12 | Brand card is pure text | 96/36/28px ✓ |

Every overlay ≤5 words, lives in the reserved lower-third with scrim, and never overlaps the dock's typing
region at the specified camera focuses (verified per-scene: at S1/S6 focus, the active typing row sits in the
upper 60% of frame). **PASS.**

## 6. Designated trims & stretch (pacing insurance)

- **Trim 1**: S7 → 75f (drop the scroll, hold the painted hunk). Saves 30f.
- **Trim 2**: S2 → 90f (pull-back 45f + shorter hold). Saves 15f.
- **Trim 3**: phone PiP window inside S6 can shrink to 45f (f630–675) without losing the mirror beat.
- **Never trim**: S1 hook, S5/S9 dialogs (interaction readability floors), S10 waves.
- Stretch (if a 60s cut is wanted): +90f in S10 (a third wave), +30f hold on S11.
