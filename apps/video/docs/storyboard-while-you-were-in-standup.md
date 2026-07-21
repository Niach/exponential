# Storyboard — "While You Were in Standup"

Build-ready board for the Remotion launch video. 1920×1080 · 30fps · **1470 frames (49.0s)**.
Concept: winner-3 ("While You Were in Standup"). Everything below is recreated UI — no screenshots.

---

## 0. How this board answers the judges

| Judge flag | Fix in this board |
|---|---|
| **Rewind device is a comprehension gamble** | The huge "9:04 AM." hook timestamp *physically flies into* the corner clock chip while its digits roll backward 9:04→9:01 next to a ◂◂ glyph, under a full-width "3 minutes earlier" card held 38 frames (1.3s). The viewer watches the big number become the persistent clock — the clock is introduced at frame 6, not mid-video. |
| **Feasibility: ~10 surfaces / 3 form factors** | Consolidated to **one desktop shell built once** (hosts scenes 1–5, 8, 9 by swapping the sidebar tool window), **one cropped web panel** (agent-session card — no web sidebar/app-shell build; the inbox card reuses the same BrowserWindow), and **one phone frame** ported from the marketing MobileDemo. Scene 8's split composite is pure layout of two components that already exist by then — zero new builds after frame 795. Net-new hard builds: branch flow graph + web steer panel, exactly the two the judges priced in. |
| **52s length / typing-heavy mid-sag** | Cut to 49.0s. The terminal beat prints CLI output lines *whole* (as the real `claude` CLI does) — only the `$ git worktree add…` command types char-by-char (0.5 f/char, 21 frames). No beat holds a static frame longer than 45 frames without new motion. The first 15s (hook → rewind → release click) works as a standalone loop. |
| **Platform/sync story nearly absent** | The phone-steer beat is staged as an explicit *sync* beat: the message is typed on the phone, and the hard cut shows the same message landing in the web feed with the secondary caption "synced to every screen, live". Coding pills (S1) and PR chips (S8) land "by themselves" — Electric-sync moments, noted in the build notes so the motion reads server-echo, not local click. |
| **Claude brand coupling** | "Claude" appears only in real product strings: dock tab `claude · release v1.0`, web/mobile badge lines, `Claude running`. No overlay copy names Claude. |

---

## 1. Scene table (30fps, frame-exact)

| # | Scene | from | durationInFrames | Transition IN | Transition OUT |
|---|---|---:|---:|---|---|
| 1 | Hook — the board drives itself | 0 | 90 | Cold open (2f fade from black) | Hard cut (continuous surface into S2) |
| 2 | Rewind — 3 minutes earlier | 90 | 60 | Continuous (same surface) | Hard cut |
| 3 | Release launch — one click | 150 | 180 | Hard cut | **Match cut**: dialog collapses toward dock, camera keeps moving into S4 |
| 4 | Orchestrator terminal | 330 | 165 | Match cut (camera continues push) | Hard cut |
| 5 | Branch flow graph | 495 | 135 | Hard cut | 8f crossfade |
| 6 | Web steer view — live diffs | 630 | 165 | 8f crossfade | Phone slides over (S7 overlays S6's frozen frame) |
| 7 | Phone steer + web echo | 795 | 165 | Slide-over (S6 blurs beneath) | Hard cut |
| 8 | PR cascade composite + time-lapse | 960 | 150 | Hard cut | Hard cut |
| 9 | 8 of 8 → Merge → Shipped | 1110 | 180 | Hard cut | Hard cut |
| 10 | Inbox card → brand outro | 1290 | 180 | Hard cut | 12f fade to black (frames 1458–1470) |

Total = 90+60+180+165+135+165+165+150+180+180 = **1470 frames**.

---

## 2. Global systems (used by every scene)

### 2.1 Stage & camera convention
- **Desktop IDE stage**: one component, 1536×960 design px (radius 10, 1px white@10% border), centered on the canvas over `Background` (the #09090b canvas + two drifting indigo radial glows from `apps/video/src/components.tsx`). Shell per desktop-ide.md: 38px topbar (project pill "Exponential" w/ indigo `code` glyph, run select "Dev Server" + green play, git cluster `⎇ main`), 44px rail, **sidebar widened to 520px** (legit — draggable 180–520) so tool-window crops fill the frame, terminal dock bottom.
- **Camera** = wrapper transform on the stage. Specified as `scale` + `focus(x, y)` in stage coordinates (the stage point that lands on canvas center): `translate = canvasCenter − focus × scale`.
- **Easings** (names used below): `EASE` = cubic-bezier(0.16,1,0.3,1) (expo-out — all entrances/camera pushes); `easeInOut` = cubic-bezier(0.65,0,0.35,1) (camera pans/flights); `linear` (drifts, scrubs); `springPop` = Remotion spring {damping 12, mass 0.6, stiffness 180} (glyph/pill/check pops, ~1.12 overshoot); `springSettle` = {damping 18, stiffness 160} (phone landing).

### 2.2 Persistent clock chip (the narrative spine)
- Docked top-right at canvas (x 1748, y 40), above everything. Pill: h 34px, rounded-full, bg #171717 @ 92%, 1px border white@10%, padding 6 14, `clock` lucide 14px #a1a1a1 + time in **JetBrains Mono 17px #fafafa, tabular-nums** (e.g. `9:01 AM`).
- Born in S2 (the hook timestamp flies into it), dies at S10 outro (12f fade at frame 1362).
- Digit changes = odometer roll: old digit slides up 100%/new slides in from below, 4 frames, `EASE`, 1.5px motion blur.
- Readings per scene: S2 9:04→9:01 · S3 9:01→9:02 · S4 9:02 · S5 9:04 · S6 9:07 · S7 9:09 · S8 **9:12→9:41 digit-blur roll** · S9 9:45→9:47 · S10 9:47.

### 2.3 Overlay type system (all captions)
- **Primary caption**: Inter 600, 40px, #fafafa, letterSpacing −0.5, centered, y = 950 (lower third), text-shadow `0 2px 28px rgba(0,0,0,0.85)`. Enter: fade + 16px rise, 12f, `EASE`; exit: 8f fade. One line max, always on ≥ 55 frames.
- **Secondary caption**: Inter 500, 24px, #a1a1a1, centered, 44px below primary.
- **Hook timestamp** (S1 only): Inter 700, 112px, #fafafa — see S1.
- **Title card** (S2 only): Inter 700, 48px, #fafafa, centered at y 540 over a full-width scrim band (black@55%, 160px tall, 40px feathered edges).

### 2.4 Cursor
Recreated macOS pointer, 24px white with dark outline + soft shadow, moves on `easeInOut` paths (18–24f per move). Click = scale dip 1.0→0.92→1.0 over 4f + 20px radial pulse (white@35% → 0) over 8f.

---

## 3. Shared fixture — the ONE dataset (cross-scene continuity)

**Workspace** `Exponential` · **Project** `Exponential` (type dev, color #6366F1, prefix EXP) · **Repo** `Niach/exponential` (default branch `main`).
**People/devices**: Dennis Strähhuber (`DS`, danny@exponential.at) on **MacBook Pro** (`dennis-mbp`); Ada Lindqvist (`AL`) appears only as a watcher in the web presence strip.
**Release**: `v1.0 – Launch` · Target Jul 15 · 8 issues · integration branch `exp/rel-v1-0` · release PR **#222** `Release v1.0 → main`.

**The 8 release issues** (order = board order top→bottom in S1; every scene draws from this table):

| Identifier | Title | Priority | Label | Assignee | Wave | Branch | PR |
|---|---|---|---|---|---|---|---|
| EXP-142 | Diff viewer drops renamed files from the file list | Urgent ⚠ #EF4444 | bug #EF4444 | DS | 1 | exp/EXP-142 | #216 |
| EXP-138 | Steer composer loses focus after Esc interrupt | High ↑ #F97316 | bug | DS | 1 | exp/EXP-138 | #214 |
| EXP-141 | Release progress bar counts cancelled issues | Medium = #FACC15 | ux #3B82F6 | — | 1 | exp/EXP-141 | #215 |
| EXP-143 | Terminal dock tab order jumps on reconnect | Medium | bug | — | 2 | exp/EXP-143 | #217 |
| EXP-144 | Branch graph lanes overlap beyond six worktrees | Low ↓ #3B82F6 | ux | — | 2 | exp/EXP-144 | #218 |
| EXP-149 | Release dialog: add a target-date picker | Medium | feature #22C55E | DS | 2 | exp/EXP-149 | #219 |
| EXP-146 | Notification digest skips @mentions in comments | High | bug | — | 3 | exp/EXP-146 | #220 |
| EXP-147 | Onboarding repo picker stalls on empty orgs | Medium | bug | — | 3 | exp/EXP-147 | #221 |

**Terminal script** (S4, verbatim, ✓/●/$ grammar):
```
✓ Read 8 issues from release v1.0 – Launch
● Planning dependency waves…
  Wave 1  EXP-138 · EXP-141 · EXP-142   (parallel)
  Wave 2  EXP-143 · EXP-144 · EXP-149   ·   Wave 3  EXP-146 · EXP-147
$ git worktree add ../EXP-142 exp/EXP-142
● Spawning subagent · EXP-142 — diff viewer rename fix
```

**Web steer feed script** (S6, in stream order; Sparkles = narration bubble, Wrench = tool row w/ mono detail):
1. 🗨 "Starting on EXP-149 — the release dialog needs a target-date field."
2. 🔧 `Read` — `apps/web/src/components/create-release-dialog.tsx`
3. 🔧 `Edit` — `create-release-dialog.tsx`
4. 🗨 "Sketching a small custom calendar grid for the date field."
5. 🔧 `Write` — `components/release-date-calendar.tsx`
6. 🔧 `Bash` — `bun run typecheck`
- Pinned "Latest changes" strip: `+86 −12` → odometer → `+214 −47`.

**Steer exchange** (S7): user capsule (right-aligned) `Reuse the existing DatePicker component` → 🗨 "↳ Got it — dropping the custom grid, wiring the shared DatePicker instead." → 🔧 `Edit` — `create-release-dialog.tsx` → Latest changes ticks `+214 −47` → `+142 −61` (net *down* — honest detail: the custom file got deleted).

**Notification** (S10): icon circle `Rocket`; line 1 `Release v1.0 – Launch shipped` + `just now` + unread dot; line 2 `PR #222 merged into main · 8 issues shipped`.

**Chips/pills vocabulary**: "coding" pill = rounded-full, 1px border #22C55E@40%, 6px green dot + `coding` 12px; PR chip = same shape, `PR #216` JetBrains Mono 12px #22C55E; "Shipped" pill = green text on green@40% border. All per desktop-ide.md §5.4.

---

## 4. Scenes

---

### SCENE 1 — Hook: the board drives itself (frames 0–89, 3.0s)

**Surface**: Desktop IDE issue-board tool window (sidebar at 520px), cropped so only the "In Progress" group header + the 8 fixture rows + a sliver of the rail's left edge are visible. Rows show: priority glyph · mono id · status glyph · title (truncated) · 1 label chip · avatar/dashed-circle. Due-date column cropped out.

**Camera**: | frame | scale | focus (stage x,y) | easing |
|---|---|---|---|
| 0 | 2.40 | (300, 300) | — |
| 89 | 2.28 | (306, 296) | linear (slow breathing pull-out) |

**Live state & micro-animations**:
- f0: all 8 rows show **backlog** `circle-dashed` glyphs (#A1A1A1), titles at full brightness, group header reads `⏱ In Progress` on yellow tint rgba(234,179,8,.10) with count `0` (13px muted, tabular).
- f8 onward, **3-frame stagger per row** (row order = fixture order): status glyph morphs `circle-dashed` → `timer` #FACC15 — 4f crossfade + `springPop` scale 1.0→1.18→1.0; simultaneously a green **"coding" pill** slides in at the row's right edge (translateX 24→0 + fade, 8f, `EASE`). These land row-by-row with no cursor anywhere — server-echo motion (Electric sync), which IS the hook.
- Header count odometer-ticks 0→3 (f20) → 6 (f32) → 8 (f44), 4f rolls.
- f44–f89: hold; pills' green dots pulse (opacity 1→0.55→1, 36f period); one row (EXP-142) gets a subtle hover-less repaint shimmer at f60 (bg accent@30% flash, 6f) to keep the frame alive.

**Overlays** (verbatim):
- f6: `9:04 AM.` — Inter 700 **112px** #fafafa, centered, y 380 (fade + 20px rise, 10f, `EASE`).
- f14: `Nobody's at the keyboard.` — Inter 600 **44px** #fafafa, centered, y 500 (fade + 16px rise, 10f).

**Build complexity**: MEDIUM. Needs: IDE board tool window (ports from marketing `src/ide` issue rows), glyph-morph component, coding pill, odometer count. Hard part: nothing exotic — this scene's row component is reused by S3/S8/S9. The 520px sidebar variant means one width prop, not a second board.

**Sound-off check** ✅: motion (self-flipping rows) + the two-line claim carry everything; zero audio dependence. Overlay on screen 83 frames.

---

### SCENE 2 — Rewind: 3 minutes earlier (frames 90–149, 2.0s)

**Surface**: same board, continuous.

**Camera**: holds S1's end value (scale 2.28, focus (306,296)) — frozen during scrub.

**Micro-animations**:
- f90–f114 (**24-frame reverse scrub**): S1's row-flip timeline plays backward at 3.75× (map scrub frame → S1 internal frame 89→0). Rows un-flip, pills retract, count rolls 8→0. Motion feel: 3px vertical directional blur on row content during the scrub (two offset 40%-opacity copies), plus a 1-frame white@4% full-frame flicker at f90 and f114 (VCR blink).
- **Timestamp flight** f92–f110 (`easeInOut`): the S1 "9:04 AM." lockup shrinks 112px→17px and flies from (960,380) to the clock-chip slot (1748,40); the chip pill (bg/border) fades in under it f104–f112. During flight the minute digits odometer-roll 4→3→2→1 (one roll per 4f). A **◂◂ rewind glyph** (18px #a1a1a1) sits left of the chip f92–f130, then fades.
- The `Nobody's at the keyboard.` line exits f90–f96 (fade).

**Overlays**:
- f104: title card `3 minutes earlier` — Inter 700 **48px** #fafafa on the 160px black@55% scrim band, y 540. Holds through f142 (38 frames ≥ 1.25s), exits 8f fade.

**Build complexity**: LOW-MEDIUM. Reuses S1 entirely (drive S1's timeline with a reversed frame map). New: clock chip + flight path + scrim card. Hard part: the timestamp flight must visibly *become* the chip — same text node, interpolated fontSize/position, no crossfade between two elements.

**Sound-off check** ✅: reverse motion + ◂◂ + backward-rolling digits + explicit title card = quadruple-redundant "we went back in time". This is the judges' fix.

---

### SCENE 3 — Release launch: one click before standup (frames 150–329, 6.0s)

**Surface**: full desktop shell. Rail active tool = Releases (`rocket` icon tinted #6366F1 + 2px accent bar). Sidebar shows the Releases tool window → release detail → the release Start-coding dialog over the shell.

**Camera**: | frame (scene-local) | scale | focus | easing |
|---|---|---|---|
| 0 | 1.00 | (768, 480) — full shell | — |
| 70→85 | 1.00→1.18 | → (768, 460) (dialog center) | `EASE` |
| 150→180 | 1.18→1.34 | → (700, 720) (drifting toward the dock strip) | `easeInOut` — continues into S4 |

**Live state & beats** (scene-local frames):
- f0–f8: shell settles (2f cut-in; glow drifts). Releases list shows one row: `🚀 v1.0 – Launch` / sub-line `Target Jul 15 · 0 of 8 done`. Clock reads 9:01.
- f8–f30: cursor enters from right, clicks the rocket row (f26).
- f30–f48: **release detail** slides in (translateX 40→0 + fade, 12f, `EASE`): back chevron + name row, summary block (rocket + `v1.0 – Launch` 13px semibold; meta chips `📅 Target Jul 15`), **4px progress bar at 0%** + right label `0 of 8 done`, issue checklist below grouped under `◌ Backlog 8` (all 8 fixture rows, dimmed).
- f48–f66: cursor moves to header `▶ Start coding` (green play glyph), clicks f64.
- f66–f80: **dialog** scales up (0.92→1.0 + fade, 10f, `springSettle`; scrim black@50%): 560px, radius 8, #171717, title `Start coding on release`. Contents top→bottom: intro line "One Claude orchestrator implements the checked issues of “v1.0 – Launch” — one subagent per issue." (12px muted); repo group header `Niach/exponential`; **8 checklist rows** (checkbox + mono id + title).
- f82–f98: checkboxes tick in a **2-frame cascade** (each: `springPop` scale 0→1.15→1, 6f, green check).
- f100–f112: Model select reads `Fable`, Effort `Max`, Subagent model/effort `Inherit` (already set — no interaction needed); cursor flips the **`Dynamic workflows (ultracode)`** switch (f106): thumb slides 10f, track tints #4F46E5, one glow pulse (box-shadow 0 0 12px rgba(79,70,229,.6), 8f in/out). Caption under it (verbatim): "Runs the orchestrator with --effort ultracode — works with any model."
- f118–f134: cursor clicks primary **`Start coding`** (f126) → label swaps to `Starting…` + 12px spinner.
- f150–f180: **dialog collapse match cut**: dialog scales 1.0→0.06 with `easeInOut`, translating toward the dock strip's tab position (stage ≈ (620, 905)); shell dims 10% behind; clock rolls 9:01→9:02 at f160.

**Overlay**: f36: `One click before standup.` — 40px primary caption, holds to f150 (114 frames).

**Build complexity**: HIGH (the scene that pays for the shell). Needs: full IDE chrome, Releases list + detail (progress bar, meta chips), the 560px release dialog (checklist, two select rows, switch), cursor. Hard parts: dialog checklist cascade timing against cursor choreography; the collapse-toward-dock match cut must hand its camera velocity to S4 frame 0.

**Sound-off check** ✅: rocket row → green play → big dialog → "Start coding" click is a legible cause-and-effect chain; caption names the meeting stake.

---

### SCENE 4 — Orchestrator terminal (frames 330–494, 5.5s)

**Surface**: desktop shell, bottom terminal dock. Dock tab strip: **`claude · release v1.0`** (active, spec-exact label) + `zsh` + `+`. Grid: JetBrains Mono 13px / line-height 1.3 on #0A0A0A.

**Camera**: | frame | scale | focus | easing |
|---|---|---|---|
| 0 | 1.34 | (700, 720) — inherited from S3 | — |
| 0→20 | 1.34→1.70 | → (620, 790) (dock fills lower ⅔ of canvas) | `EASE` |
| 20→165 | 1.70 | hold (glow drift only) | — |

**Micro-animations** (scene-local):
- f0–f14: dock expands 29px→240px (`EASE` height tween); the S3 dialog-collapse dot lands on the new tab at f4 (6px green flash on the tab label).
- Script lines (see fixture §3). CLI-realistic delivery: **✓/●/wave lines print whole** (per-line: fade + 6px rise, 4f), gaps 10 frames; **only the `$` command types char-by-char at 0.5 frames/char** (42 chars ≈ 21 frames) behind a solid #FAFAFA block cursor:
  - f16 line 1 ✓ · f30 line 2 ● (+ braille spinner ⠋⠙⠹ cycling 3f/glyph on the bullet while "Planning…") · f44 line 3 · f56 line 4 · f68–f89 line 5 types · f96 line 6 prints.
- f96–f165 hold (69f): block cursor blinks (16f period) on line 6; spinner keeps cycling; line 6's `EXP-142` briefly highlights (accent@40% bg, f120, 8f). Every line is on screen ≥ 69 frames (≥ 2.3s) — no unreadable scroll.

**Overlay**: f30: `It plans the waves.` — 40px primary caption, holds to f150.

**Build complexity**: MEDIUM. Needs: dock chrome + tab strip (ports from marketing recreation's typed session), line-printer with two delivery modes, spinner. Hard part: nothing — this is the most-proven component in the repo (marketing `src/ide` already ships a scripted session).

**Sound-off check** ✅: wave lines literally spell the plan (`Wave 1 … (parallel)`); caption reinforces. Mono at 13px × 1.70 camera ≈ 22px rendered — phone-legible.

---

### SCENE 5 — Branch flow graph: one worktree per issue (frames 495–629, 4.5s)

**Surface**: Source Control tool window (branch flow graph, desktop-ide.md §5.6) in the 520px sidebar, cropped full-frame. Rail active = Source Control (`git-merge`).

**Camera**: | frame | scale | focus | easing |
|---|---|---|---|
| 0 | 2.00 | (300, 330) | — |
| 135 | 2.06 | (300, 344) | linear (creep down as lanes sprout) |

**Live state & micro-animations**:
- f0: only `main ✓` lane visible (label medium weight, check glyph).
- f6–f24: **`exp/rel-v1-0` lane self-draws** off main: the 1px muted@35% connector rail animates stroke-dashoffset over 18f (`EASE`); label fades in mono 12px at f18 with a 6px green PR dot.
- f28–f70: three **`exp/EXP-*` lanes sprout** (14f stagger): EXP-138 (f28), EXP-141 (f42), EXP-142 (f56) — each: connector draw 12f + label fade + yellow `timer` indicator `springPop` + muted `worktree` tag.
- f76–f120: **↑ push counts tick** on the lanes (mono muted): EXP-138 `↑1`→`↑2` (f76, f100); EXP-142 `↑0`→`↑3`→`↑7` (f80, f96, f114) — each tick a 4f odometer roll + 1-frame lane-row accent@25% flash (server-echo feel).
- Clock rolls 9:02→9:04 at f20.

**Overlay**: f24: `One worktree per issue.` — 40px primary caption, holds to f120.

**Build complexity**: HIGH (net-new #1, as judges priced). Needs: lane tree renderer (1px rails in 14px gutter columns, 24px rows), stroke-draw animation, indicator glyph set. Hard part: connector geometry that survives the sprout stagger without reflow — pre-compute the final tree layout, animate reveals only.

**Sound-off check** ✅: tree shape + `worktree` tags + caption state the architecture in one image; push counts ticking = "it's really working".

---

### SCENE 6 — Web steer view: real diffs, streaming live (frames 630–794, 5.5s)

**Surface**: `BrowserWindow` chrome (radius 16, traffic lights, pill URL `app.exponential.at`) at 1280×840 design px on the indigo-glow `Background`. Inside: **cropped agent-session panel only** (web-app.md §7) — header badge row + h-96 feed panel + Latest-changes strip + steer composer. No web sidebar/app shell (deliberate feasibility cut).

**Camera** (on the BrowserWindow group): | frame | scale | focus | easing |
|---|---|---|---|
| 0→24 | window entrance: rise 46→0px, scale 0.97→1.0 | — | `EASE` |
| 0→165 | 1.00→1.06 slow push, centered | — | linear |

**Live state & micro-animations**:
- Header (visible from f0): outline badge `● Coding now` — emerald-500 dot with emerald-400 ping, **breathing at 1.2s period (36 frames)**, border emerald-500/40 — next to muted `Dennis · MacBook Pro`; phase dot + `Live · MacBook Pro`.
- Feed streams **bottom-up** (items push older ones up 8f `EASE`; each entrance: fade + 16px rise, 10f). Fixture script §3 items land at f18, f36, f52, f72, f92, f110. Tool rows: `Wrench size-3` + name (text-xs medium) + mono detail truncated. Narration: Sparkles glyph + bg-muted/30 bubble.
- Presence strip: `👁 Ada` + `⌨ Dennis (steering)` (foreground) — static.
- **Latest changes strip** (pinned): `Latest changes` + right-aligned mono — odometer-ticks `+86` emerald-400 ` −12` rose-400 → `+214 −47` at f100 (per-digit 4f rolls).
- Composer at bottom: placeholder `Message the agent…`, mono `Esc` button, ArrowUp send — idle (sets up S7).
- Clock rolls 9:04→9:07 at f16.

**Overlay**: f28: `Real diffs, streaming live.` — 40px primary caption, holds to f150.

**Build complexity**: HIGH (net-new #2). Needs: BrowserWindow (exists in `apps/video`), badge/ping, feed items (2 kinds), odometer strip, composer. Hard part: bottom-anchored feed layout (justify-end + push-up transform) — build as an absolutely-positioned stack with precomputed y offsets per frame window.

**Sound-off check** ✅: pulsing "Coding now" + streaming rows + growing +/− numbers read as live work; caption anchors the claim. The narration line "custom calendar grid" plants S7's correction for attentive viewers, but S7 works without it.

---

### SCENE 7 — Phone steer + web echo (frames 795–959, 5.5s)

**Surface A** (f0–f109): S6's final frame frozen beneath (blur 0→8px + dim to 55%, 12f); **phone frame** (330px design width, MobileDemo port: 44px outer radius, dynamic island, status bar `9:09`, zinc gradient #09090B→#18181B) staged at scale 2.2 (≈726px tall column), right-of-center (canvas x 1180).
**Surface B** (f110–f164): hard cut back to the S6 web panel, crisp, full frame.

**Camera**: static both halves; the phone itself animates.

**Phone live state**: Agents surface — floating capsule nav at bottom (5 tabs, Agents active w/ white 12% capsule + 8px green live dot); glass session card: robot glyph + `Claude on dennis-mbp` (15pt medium white) + `EXP-149 · Release dialog: add a target-date picker` (12pt white50 mono id) + **`● Live`** capsule (#34d399 dot + text); below, 3 mirrored feed lines from S6 (glass rows, 12pt): the last is "Sketching a small custom calendar grid…". Bottom: steer composer glass capsule, placeholder `Type to steer…`, green send circle.

**Micro-animations** (scene-local):
- f0–f22: phone **slides up from bottom-right at 12° tilt** (translateY 900→0, rotate 12°→0°, `EASE`), settles with `springSettle` (−4px overshoot, 8f). Clock chip stays on top, rolls to 9:09 at f10.
- f28–f76: composer **types `Reuse the existing DatePicker component`** (38 chars at **1.2 frames/char** ≈ 46f) behind a 2px iOS caret blinking 16f period; send button pulses (scale 1→1.12→1, `springPop`) at f80.
- f84–f94: message **capsule flies up** out of the composer (translateY 0→−90 + scale 0.9, fade at end, `EASE`) — send gesture.
- **f110 hard cut to web**: the same text lands in the web feed as a right-aligned user capsule (bg accent, rounded-md, `springPop` entrance) at f112 → f126 narration reply "↳ Got it — dropping the custom grid, wiring the shared DatePicker instead." → f142 tool row `Edit create-release-dialog.tsx` → f150 Latest changes odometers `+214 −47` → `+142 −61`.

**Overlays**:
- f30: `You just… steer.` — 40px primary caption (persists across the cut, holds to f150).
- f114: secondary caption `synced to every screen, live` — 24px #a1a1a1 (the platform-story beat), holds to f156.

**Build complexity**: MEDIUM-HIGH. Needs: phone frame + capsule nav + session card + composer (all port from marketing MobileDemo), typed-text caret, capsule-fly. Hard part: making the phone message and the web capsule read as *the same object* — identical string, matching capsule radius, ≤ 4 frames between fly-off and landing.

**Sound-off check** ✅: thumb-typed message → same words appear on the desktop-sized screen = sync demonstrated, not claimed; both captions carry it silently.

---

### SCENE 8 — PR cascade + time-lapse (frames 960–1109, 5.0s)

**Surface**: split composite on `Background` — **left 46%**: board rows crop (S1 component, coding pills now showing on wave-2 issues only); **right 54%**: branch flow graph crop (S5 component, now 6 lanes); 1px white@10% divider. Both panes are crops of already-built components — layout only.

**Camera**: panes enter f0–f14 (left slides 40→0 from left, right from right, fade, `EASE`); inner contents animate; no camera moves.

**Micro-animations** (scene-local):
- f8–f40: on the graph, **wave-1 lanes curve back and merge** into `exp/rel-v1-0` one by one (12f stagger: EXP-138, EXP-141, EXP-142): the lane's connector draws a return curve (stroke-draw 10f) and its `timer` indicator flips to green `circle-check` (`springPop`).
- Matching each merge (+4f echo delay, server-echo feel), the corresponding **board row pops a PR chip**: `PR #214` (f24), `PR #215` (f36), `PR #216` (f48) — mono green chips, `springPop`, replacing the coding pill; the row's status glyph flips `timer` → `circle-check` #22C55E.
- f20–f70: **clock digit-blur roll 9:12 → 9:41**: chip scales to 1.15, digits roll continuously with 3px vertical blur streaks (linear), then settles back to 1.0 (`springSettle`) — the time-lapse is *told by the clock*, oversized so it can't be missed.
- f72–f110: **wave-2 lanes sprout** on the graph as wave-1 lanes fade to 40% (EXP-143, EXP-144, EXP-149 — reuse S5 sprout anim, 10f stagger) — the machine breathing in and out.
- f80: docked **mini release progress bar** (bottom center, 420×4px green bar + label) steps `3 of 8 done` → `6 of 8 done` at f118 (fill tween 14f `EASE` + label odometer).

**Overlay**: f26: `PRs open themselves.` — 40px primary caption, holds to f130.

**Build complexity**: LOW-MEDIUM. Zero new components (board rows, graph lanes, chips, clock all exist); new work is the merge-curve path variant + the two-pane layout. Hard part: keeping the merge→chip echo rhythm readable — never two pops in the same 8-frame window on the same side.

**Sound-off check** ✅: green chips popping + merge curves + a clock visibly spinning half an hour = "PRs happened while time passed", no narration needed.

---

### SCENE 9 — 8 of 8 → Merge → Shipped (frames 1110–1289, 6.0s)

**Surface**: desktop shell, three sub-shots: (A) release detail, (B) Reviews tool window, (C) release detail header. Rail active flips Releases → Reviews → Releases accordingly.

**Camera**: | frame | scale | focus | easing |
|---|---|---|---|
| 0 | 1.90 | (300, 330) — release detail crop | — |
| 70 | hard sub-cut | (300, 300) — Reviews crop, scale 1.90 | — |
| 150 | hard sub-cut | (300, 200) — release header crop, scale 2.10 | — |

**Beats** (scene-local):
- **A (f0–f69)**: release detail — checklist rows flip to green filled check-circles in a 3f stagger (8 rows, f6–f27, `springPop` each); the **4px bar fills 75%→100%** (f30–f50, `EASE`) with a soft green glow sweep (60px white@20% gradient, L→R, 12f); label odometers to **`8 of 8 done`** (f50). f56: a new meta chip fades in: `⑂ PR #222 · open` (green PR glyph). Clock rolls 9:41→9:45 at f10.
- **B (f70–f149)**: Reviews tool window, one row:
  `⑂ Release v1.0 → main` / sub-line `#222 · exp/rel-v1-0` + outline **`Merge`** button. Cursor clicks f92 → button morphs to danger-red **`Confirm merge`** (6f crossfade) → holds 22f (readable) → click f120 → `Merging…` + spinner (12f) → row fades to 40% with **`Merged ✓`** in green (f138, `springPop` on the check).
- **C (f150–f180)**: release header — **`Shipped`** pill springs in beside `v1.0 – Launch` (`springPop`, green text/border); rocket icon tints green; clock rolls 9:45→**9:47** at f158.

**Overlay**: f74: `One release PR. You merge it.` — 40px primary caption, holds to f168.

**Build complexity**: MEDIUM. Reviews two-click merge ports from the marketing recreation; release detail reuses S3. Hard part: three sub-cuts in 6s — each sub-shot must open on motion within 6 frames so none reads static.

**Sound-off check** ✅: bar hitting 100% → a single Merge row → two clicks → `Merged ✓` → `Shipped` is a complete silent syllogism; caption names the human's only job.

---

### SCENE 10 — Inbox card → brand outro (frames 1290–1469, 6.0s)

**Surface A** (f0–f59): the S6 BrowserWindow (same chrome, URL pill `app.exponential.at`) cropped to the **web inbox card** on #0a0a0a, `Bell` + `Inbox` header above it.
**Surface B** (f60–f180): brand outro on `Background`.

**Micro-animations** (scene-local):
- f0–f14: inbox card rises in (fade + 24px rise, `EASE`): rounded-md border card — muted icon circle w/ `Rocket` glyph, line 1 `Release v1.0 – Launch shipped` (text-sm, font-medium) + `just now` (muted) + **unread dot** (h-2 w-2 #e5e5e5); line 2 `PR #222 merged into main · 8 issues shipped` (text-xs muted).
- f22: unread dot **blinks once** (opacity 1→0.2→1, 10f).
- Clock reads 9:47; it fades out f60–f72 (its story is done).
- f60–f72: **12f crossfade** to outro.
- Outro build: f66–f92 the **real cut-curve disc logo** (white disc, viewBox 0 0 100 100, three exponential bezier cut strokes width 6 — exact paths from video-brand.md §3) draws in: disc fades up 8f, then the three curves **mask-wipe** (stroke-dashoffset, 14f, 4f stagger, bottom curve first); f92: wordmark `Exponential` (Inter 700, 96px, ls −3, #fafafa) fades up beside it (gap 28px); f104: tagline `Issue tracking that ships code.` (Inter 500, 34px, #a1a1a1); f110: `exponential.at` (Inter 500, 26px, #818cf8) — 6-frame staggers.
- f110–f168: **hold 58 frames (~2s), everything static except ambient glow drift** — the loop-friendly end card.
- f168–f180: 12f fade to black.

**Overlay**: f8: `Meeting's over. Release shipped.` — 40px primary caption over the inbox card, exits in the f60 crossfade. (Outro text above is part of the brand card, not a caption.)

**Build complexity**: LOW. Inbox card = one bordered flex row inside the existing BrowserWindow; logo SVG is spec-exact copy-paste; mask-wipe is a dashoffset tween. Hard part: none — deliberately cheap closer.

**Sound-off check** ✅: notification card states the outcome in product UI; caption lands the joke; end card gives name + URL. Fully silent-readable.

---

## 5. Build-order recommendation (feasibility sequencing)

1. **Systems**: Background, clock chip, overlay type, cursor, camera rig (day 1 — unblocks everything).
2. **Desktop shell + board tool window** (S1/S2/S8 done; ~60% of screen time).
3. **Terminal dock session** (S4) — highest-confidence port from marketing recreation.
4. **Releases detail + release dialog + Reviews** (S3/S9).
5. **Branch flow graph** (S5/S8) — net-new #1.
6. **Web agent-session panel + inbox card in BrowserWindow** (S6/S7/S10) — net-new #2.
7. **Phone frame + steer composer** (S7) — MobileDemo port.
8. Composite layouts (S8), outro (S10), polish pass on match cuts (S3→S4) and the timestamp flight (S1→S2).

## 6. Sound-off readability — full-video verification

- Every scene carries exactly one primary caption (40px, ≥ 55 frames on screen) plus at most one secondary; no beat depends on audio, VO, or SFX.
- The story is redundantly encoded three ways: captions (words), the clock chip (time arithmetic 9:01→9:47), and countable UI state (0 of 8 → 3 → 6 → 8 of 8 → Merged ✓ → Shipped).
- Smallest persistent text = the 17px clock chip (safe); smallest narrative text = terminal mono 13px rendered ≈ 22px at camera scale 1.70 and feed mono ≈ 14px at window scale — both above the 12px-rendered floor for 1080p feeds; every typed/printed line holds ≥ 45 frames after completion.
- The first 15 seconds (S1–S3 through the dialog click at ~0:11) form a standalone loop: hook claim → rewind → the one human click.
- Rewind comprehension is quadruple-redundant (reverse motion, ◂◂ glyph, backward digit roll, title card) — a viewer who misses all four still recovers via the corner clock in every later scene.
