# STORYBOARD — "STRIKE THE DON'T" (board v2, build-ready)

Remotion launch film for Exponential. **1920×1080 · 30 fps · 1200 frames · 40.0 s.**
Strict 120 BPM cut grid: `BEAT = 15` frames. Every keyframe below is expressed as
`beat*15 + offset` and must be authored against the `BEAT` constant in code — never eyeballed.
Canvas: `Background` from `@exp/video` (`#09090b` + drifting indigo radial glows). All recreated
UI uses the product tokens verbatim (desktop/web `#0a0a0a / #171717 / #262626 / #fafafa / #a1a1a1`,
border `rgba(255,255,255,.10)`; mobile zinc gradient `#09090B→#18181B` glass). Fonts:
`@remotion/google-fonts/Inter` weights **400/500/600/700/800** (800 is new — the type cards need it)
+ `@remotion/google-fonts/JetBrainsMono` 400/500/700. Single easing everywhere unless named:
`EASE = cubic-bezier(0.16,1,0.3,1)` ("expoOut"). Extra named easings:
`GRAVITY = cubic-bezier(0.55,0,1,0.45)` (the DON'T drop), `LINEAR` (drifts/counters),
`SPRING_POP` = Remotion `spring({damping:12, stiffness:200})` (pills/badges, reads as 1.15→1.0 overshoot).

---

## 0. How this board answers the judges

| Judge criticism | Fix in this board |
|---|---|
| "Receipts, not a lived sense of one connected system" / "could belong to any competitor with the nouns swapped" (J1, J3) | ONE issue — **EXP-142** — is now the protagonist of the whole film. It is the issue clicked in S2, the branch/PR in S3/S5, the live session steered in S7, visible in both panes of S9, the PR merged in S11, and the 8th checkbox that ships release v0.12 in S12. A persistent mono **thread chip** (bottom-left) names the surface + the thread (`EXP-142 · DESKTOP`, `EXP-142 · WEB · LIVE`, `PR #214 · REVIEW`…) so the cross-surface continuity is *explicit*, sound-off, at feed size. The mechanisms filmed (green "Start coding" button → `claude · EXP-142` terminal tab → steer composer → release auto-progress) are product-specific; swapping nouns can't reproduce them. |
| "1.5–2.5 s dense-UI vignettes sit at the readability floor" (all three) | Vignette count cut and durations rebalanced: terminal **3.5 s**, live-steer **3.5 s**, split-sync **4.0 s**, feedback→issue **3.5 s**. Every UI scene is cropped 130–260 % with ONE focal action; terminal tool-lines pop as whole lines (authentic to the claude CLI *and* more legible than char-typing); no full 1920 px shell is ever shown uncropped. |
| "~12 surfaces is real work" (J2, J3) | Recreated-surface count drops to **9** (+ type system + outro). The solo-mobile beat is merged into the split-sync scene (which now proves sync BOTH directions — a stronger claim for fewer builds); the web issue-detail timeline beat is merged into the steer scene (breadcrumb + Details/Changes tabs are simply the top of the same crop); Reviews and Releases share one desktop tool-panel chrome; S2→S3 share one desktop shell build. Four of 15 scenes are near-free type cards; three ports come straight from the marketing recreation (`apps/marketing/src/ide`). |
| "The re-kern IS the hook and can't be janky" (all three) | S1 carries a full implementation spec (per-word spans, measured widths, keyframe table, motion-blur recipe). Budgeted as its own component with its own polish pass. |
| "Beat grid must be enforced in code" (J1) | `BEAT = 15` constant; the scene table and every micro-animation below is beat-indexed. Accents (clicks, pops, flips) land ON beat boundaries by construction. |
| "Terminal typing must be frame-driven" | `FrameTyper` util: script is a frame-indexed lookup (`charsVisible = clamp((frame - startFrame) * CPS/30, …)`), zero timers, deterministic renders. |
| "40 s is long for X autoplay" (J2) | The 0–15 s cutdown is preserved by construction: S1–S4 (frames 0–299, exactly 10 s: hook → Start coding → terminal → claim) + a 5 s outro card renders standalone with no re-editing. |

---

## 1. Scene table (frame-exact, 30 fps)

| # | Scene | from | durationInFrames | beats | Transition IN | Transition OUT |
|---|---|---|---|---|---|---|
| S1 | Hook — strike the DON'T | 0 | 75 | 1–5 | none (frame 1 is mid-slam) | hard cut on beat |
| S2 | Desktop detail — "Start coding" | 75 | 75 | 6–10 | hard cut | **none — continuous camera push into dock** |
| S3 | Claude terminal session → PR #214 | 150 | 105 | 11–17 | continuous from S2 | hard cut |
| S4 | Type — YOU WRITE / CLAUDE WRITES | 255 | 45 | 18–20 | hard cut | hard cut |
| S5 | Desktop Changes tab diff | 300 | 90 | 21–26 | hard cut | hard cut |
| S6 | Type — WATCH IT. STEER IT. | 390 | 45 | 27–29 | hard cut | hard cut |
| S7 | Web live steer (in issue-detail chrome) | 435 | 105 | 30–36 | hard cut | hard cut |
| S8 | Type — SYNCED. EVERYWHERE. INSTANTLY. | 540 | 45 | 37–39 | hard cut | hard cut |
| S9 | Split-screen two-way sync (web ⇄ phone) | 585 | 120 | 40–47 | hard cut (panes slide 10f) | hard cut |
| S10 | Type — REVIEW. MERGE. SHIP. | 705 | 45 | 48–50 | hard cut | hard cut |
| S11 | Desktop Reviews — two-click merge | 750 | 60 | 51–54 | hard cut | hard cut |
| S12 | Desktop Releases — 8 of 8 · Shipped | 810 | 60 | 55–58 | hard cut | hard cut |
| S13 | Type — FEEDBACK IN. PULL REQUESTS OUT. | 870 | 45 | 59–61 | hard cut | hard cut (no fade) |
| S14 | Widget → issue drops on the board | 915 | 105 | 62–68 | hard cut | hard cut (internal hard cut at f960) |
| S15 | Brand outro — real cut-curve logo | 1020 | 180 | 69–80 | hard cut | end (holds last frame; loops cleanly into S1) |

Total: **1200 frames = 40.0 s**. All scene boundaries land on beat boundaries (multiples of 15).

Camera convention used below: each UI scene renders its surface at a stated design size inside a
camera rig; keyframes are `(frame · scale · center-point in surface px · easing)` where the center
point maps to screen center. Cursor = macOS-style white pointer with soft shadow, moves on EASE,
click = 24px ripple ring (border `rgba(255,255,255,.5)`) expanding 0→40px over 8f + button pressed-state on the click frame.

---

## 2. SHARED FIXTURE — the one dataset every scene draws from

**Workspace**: `Exponential` (slug `exponential`). Members: **Dennis Strähhuber**
(`danny@exponential.at`, avatar initials `DS`). Widget agent user: **Exponential App** (isAgent).
Device running the coding session: **MacBook Pro** (owner Dennis).

**Project**: `Exponential` — type **dev**, color `#6366f1`, prefix `EXP`, repo `Niach/exponential`,
default branch `main`.

**Labels**: `bug` #ef4444 · `desktop` #a855f7 · `web` #3b82f6 · `feedback` #f97316.

**Hero issue** — the film's protagonist:
- **EXP-142 — "Reconnect live steer sessions after network drop"** — urgent, label `bug`,
  assignee DS, due **Jul 15**, status `in_progress` → flips `done` at f795 (PR merge).
  Branch **`exp/EXP-142`**, **PR #214**, diff **5 files +120 −34**. Member of release v0.12.

**Supporting issues** (every on-screen row comes from this list — no ad-hoc rows):

| id | title | status (start → end) | prio | labels | assignee | due |
|---|---|---|---|---|---|---|
| EXP-138 | Add effort picker to the Start-coding dialog | in_progress → done (S9, web flip, f618) | high | desktop | DS | Jul 18 |
| EXP-136 | Release progress bar clips on narrow sidebars | todo → done (S9, phone swipe, f663) | medium | web | — | — |
| EXP-146 | Widget: retry queue for offline submissions | todo | low | web | — | Jul 22 |
| EXP-139 | Terminal dock loses scroll position on tab switch | done | high | desktop | DS | — |
| EXP-131 | Hide PR fields from anonymous board viewers | done | medium | web | DS | — |
| EXP-129 | Annotation editor: undo stack for arrows | backlog | none | feedback | — | — |
| EXP-145 | Feedback: onboarding stalls on the repo picker | **created at f963** → backlog | none | feedback | — (creator: Exponential App) | — |

**Release v0.12 — "Live steer hardening"** — target **Jul 15**, 8 issues:
EXP-142 (open until f795) + 7 done: EXP-139, EXP-131, EXP-127 "Windows self-updater swaps the
running exe", EXP-125 "Steer ticket refresh on token expiry", EXP-124 "Presence chips drop on
rejoin", EXP-121 "Relay room GC for ended sessions", EXP-119 "Public viewer stream backpressure".
Progress **7 of 8 done → 8 of 8 done** (S12).

**Terminal script** (the claude session, verbatim, JetBrains Mono):
```
✓ Read issue EXP-142 — Reconnect live steer sessions after network drop
● Edit apps/web/src/components/agent-session.tsx
● Edit apps/steer-relay/src/rooms.ts
✓ bun test — 34 passed
$ git push -u origin exp/EXP-142
✓ Opened pull request #214
```
(`✓` #22C55E, `●` #F97316 claude-bullet, `$` #FAFAFA, output muted #A1A1A1.)

**Diff manifest** (sums exactly to +120 −34):
```
M  apps/web/src/components/agent-session.tsx    +46 −18
M  apps/web/src/lib/steer.ts                    +22 −9
M  apps/steer-relay/src/rooms.ts                +31 −5
A  apps/web/src/lib/steer-backoff.ts            +18 −0
M  packages/steer-ticket/src/verify.ts          +3 −2
```
Focal hunk = the all-green added file (reads instantly), containing the brand pun for freeze-framers:
```
+ export const backoffDelay = (attempt: number) =>
+   Math.min(500 * 2 ** attempt, 30_000)   // exponential backoff, capped
```

**Steer exchange** (S7): narration bubble *"Reconnect loop is in. Refreshing the steer ticket and
resubscribing the stream."* → tool row `Bash` · `bun run test:steer-relay` → Dennis types
**"add jitter to the backoff"** → send → reply bubble starts *"Adding ±20% jitter…"* (cut mid-stream — the loop is alive).

**Continuity invariants** (assert in fixtures, they cross scenes): PR number **#214** appears in
S3 (terminal), S7 chip context, S11 (Reviews row `#214 · exp/EXP-142`); the `+120 −34` stats appear
in S5 header and S7 "Latest changes" strip; EXP-138/EXP-136 are done in every scene after f663
(so S14's board correctly shows NO In Progress group — In Progress is empty after EXP-142 merges);
Backlog count ticks 1→2 when EXP-145 drops.

**Thread chip** (new component, answers J3): rounded-full pill, bg `#171717`, 1px border
`rgba(255,255,255,.10)`, JetBrains Mono **20px / 500**, `#a1a1a1`, uppercase, 48px inset bottom-left,
fades in 8f after each UI scene starts (rise 12px, EASE), cuts with the scene. Text per scene given below.

---

## 3. Scenes

### S1 — HOOK: strike the DON'T — f0–74 (75f, beats 1–5)

**Surface**: full-bleed type card on `Background` (indigo glow drifting; glow center eases +4% x
over the scene, LINEAR).

**Overlay (verbatim)** — Inter **800**, **150px**, line-height 0.95, letter-spacing −0.02em, `#fafafa`,
two centered rows:
> **ISSUE TRACKERS**
> **DON'T SHIP CODE.**
becoming, after the gag:
> **ISSUE TRACKERS**
> **SHIP CODE.** *(final period replaced by the green CircleCheck glyph)*

**Micro-animation timeline** (frame 1 is already mid-motion — no dead air):
- f0 (b1): "ISSUE TRACKERS" slams — scale 1.35→1.0 over 9f EASE, directional blur `filter: blur(8px→0)` frames 0–6 only.
- f15 (b2): "DON'T" slams (same recipe).
- f30 (b3): "SHIP CODE." slams. Line complete at f39 (1.3 s).
- f45 (b4): **strike** — 5px bar, `#EF4444`, draws left→right across "DON'T" over 6f (f45–51), LINEAR width 0→100%. (Spec'd 5px, not 3px, to survive feed compression.)
- f52: "DON'T" tips **rotate 0→8°** (transform-origin bottom-left) and **drops**: translateY 0→620px + translateX +60px over 14f, **GRAVITY**; strike bar falls with it.
- f56: **re-kern** — remaining words close the gap over 8f EASE (starts 4f into the drop, so the line "heals" while the word is still falling).
- f60 (b5): the final period pops into a **CircleCheck** glyph, `#22C55E`, sized to cap-height, SPRING_POP.
- f60–74: hold; camera micro-push scale 1.00→1.03 LINEAR across the whole scene.

**Re-kern implementation spec (hook-critical)**: each word is an absolutely-positioned `<span>`;
word widths measured once via canvas `measureText` (deterministic — bundled Inter); x-positions are
pure functions of frame: `x_i(f) = interpolate(f, [56, 64], [x_withDont, x_withoutDont], EASE)`.
"DON'T" renders in its own layer with its strike bar so rotation/drop can't disturb line layout.
Motion blur on slams = `blur(velocity * 0.9px)` capped 8px, applied frames 0–6 of each slam only.

**Camera**: static (type layer owns its transforms).
**Sound-off check** ✅ — pure kinetic type; the entire category reframe lands by f75 with zero context, zero audio.
**Build**: `TypeSlamLine`, `StrikeDrop` components. HARD: the re-kern (spec above). Everything else near-free. Reuse: `Background`, `EASE` from `@exp/video`.

---

### S2 — DESKTOP: "Start coding" on EXP-142 — f75–149 (75f, beats 6–10)

**Surface**: Desktop IDE recreation (port of `apps/marketing/src/ide`, updated fixtures), design size
**1600×1000**. Visible in crop: issue-detail **header row** (`Details Changes … 12 / 34 ˄ ˅ · ▶ Start
coding · 🔔 Subscribed`), title "Reconnect live steer sessions after network drop" (20px semibold),
top edge of the properties panel (STATUS ⏱ In Progress · PRIORITY ⚠ Urgent), and the collapsed
**29px terminal strip** (`⌸ Terminal (1)` muted, chevron-up) at the bottom. Start coding = ghost
xsmall, green `#22C55E` play glyph + "Start coding" 12px.

**Camera** (center-point in surface px):
- f75: scale **1.66**, center (1080, 300) — header row huge, slight over-zoom.
- f87: scale **1.60**, center (1080, 330), EASE — settle.
- f108–135: scale 1.60→**1.75**, center →(880, 760), EASE — begins the push toward the dock as it expands (continues into S3).

**Micro-animations**:
- f81: thread chip in: **`EXP-142 · DESKTOP`**.
- f80→f97: cursor glides in from right edge to the Start coding button (EASE).
- **f105 (b8): click** — ripple + button pressed state, exactly on the beat.
- f108→f120: **terminal dock expands 29px→240px** (12f, EASE) — the scene's ONE micro-interaction.
- f120: tab **`claude · EXP-142`** lights in the dock tab strip (JetBrains Mono 13px, active accent bg), SPRING_POP on the tab label.
- f122+: block cursor blinks in the grid (16f cycle: 8 on / 8 off, frame-derived `(f>>3)&1`).
- **f135 (b10)**: first line pops whole-line: `✓ Read issue EXP-142 — Reconnect live steer sessions after network drop`.

**Overlays**: thread chip only (no headline — the type cards carry claims).
**Sound-off check** ✅ — green play button + dock expansion + lit tab read as "code started from an issue" with no words; chip anchors which issue.
**Build**: `DesktopShell` (topbar/rail simplified — only the visible slice needs pixel fidelity), `IssueDetailHeader`, `TerminalDock`. Port `ide.css` values 1:1. HARD: nothing new — dock expansion is a height interpolate. Cursor component shared app-wide.

---

### S3 — THE TERMINAL: Claude ships it — f150–254 (105f, beats 11–17)

**Surface**: same `DesktopShell` instance — **no cut**; the S2 camera push completes so the terminal
dock fills the frame. Grid: JetBrains Mono **13px/1.3** on `#0A0A0A`, token-locked ANSI palette
(green #22C55E, claude-bullet #F97316, muted #A1A1A1). Tab strip visible on top: `claude · EXP-142` active.

**Camera**: f150: scale 1.75 → f162: scale **2.05**, center (880, 800), EASE. Then static (dock ≈ full width, ~15 grid rows visible at ~27px effective line height — comfortably legible at feed size).

**Micro-animations** (tool lines pop as WHOLE lines snapped to beats — authentic claude-CLI output and maximally legible; only the `$` command types):
- f165 (b12): `● Edit apps/web/src/components/agent-session.tsx` pops.
- f180 (b13): `● Edit apps/steer-relay/src/rooms.ts` pops.
- f195 (b14): `✓ bun test — 34 passed` pops (a 2f spinner glyph `✻` precedes it f188–194).
- f210 (b15): `$ git push -u origin exp/EXP-142` **types at 2 chars/frame** (32 chars ≈ 16f, done f226).
- f232: muted output line ` → origin exp/EXP-142` pops.
- **f240 (b17): `✓ Opened pull request #214`** pops with **scale 1.12→1.0** (6f EASE) — the payoff line.
- f247: **exit-code-0 badge** stamps onto the tab (green@15 bg, green text, SPRING_POP) and the exit strip slides up 8f: `● Process finished with exit code 0` (6px green dot, 12px muted).

**Overlays**: thread chip updates at f150: **`EXP-142 · CLAUDE · exp/EXP-142`**.
**Sound-off check** ✅ — ✓/●/$ line grammar + green pop on "Opened pull request #214" + exit-0 badge: the "issue → PR" story is readable from glyph colors alone.
**Build**: `ScriptedTerminal` — port the marketing 8-line script engine, replace `setTimeout` with the frame-indexed `FrameTyper`. HARD: nothing; determinism rule is the whole trick.

---

### S4 — TYPE: the claim — f255–299 (45f, beats 18–20)

**Overlay (verbatim)** — two stacked centered lines, Inter **800**, **92px**, ls −0.02em:
> **YOU WRITE THE ISSUE.** *(f255, slams white #fafafa)*
> **CLAUDE WRITES THE CODE.** *(f270, slams; the word "CLAUDE" tinted `#6366f1`)*

- Slam recipe identical to S1 (scale 1.35→1.0, 9f, blur 0–6f).
- f285 (b20): subtle radial glow pulse behind line 2 (`rgba(99,102,241,.25)`, scale 1→1.15→1, 12f).

**Camera**: static. **Sound-off** ✅ — the claim IS the audio track.
**Build**: reuses `TypeSlamLine`. Free.

---

### S5 — DESKTOP: the diff receipt — f300–389 (90f, beats 21–26)

**Surface**: Desktop **Changes tab** (full-width variant), design 1600×1000, cropped **1.7×**. Header
(verbatim): `⎇ exp/EXP-142` (mono #FAFAFA) · `● Claude running` (6px green dot + muted) ·
`Local — includes uncommitted` (muted) · `5 files` `+120` (#22C55E) `−34` (#EF4444). Left: 240px file
list (the 5 fixture files, mono 12px, status letters M yellow / A green). Right: side-by-side diff —
18px rows (≈31px effective at 1.7×), green/red @10% line tints, blue-tinted `@@ -112,9 +112,14 @@`
hunk header, focal file **`steer-backoff.ts`** (all-green added file) with the
`Math.min(500 * 2 ** attempt, 30_000)` line centered.

**Camera**: f300: scale 1.70, center (1000, 420); drift translateX −40px surface-space over f300–389 LINEAR (slow lateral read). No scale change.

**Micro-animations**:
- f300–330: header counters **tick 0→120 / 0→34** (tabular-nums, LINEAR).
- f306: diff rows cascade top→bottom, **1-frame stagger**, each row's tint sweeps left→right over 6f.
- f330 (b23): file-list rows' status letters pop in sequence (2f stagger).
- f345 (b24): hunk header underline pulse (blue @80%, 6f).
- f375 (b26): the focal `+ Math.min(500 * 2 ** attempt, 30_000)` row flashes 1f at green@25% then settles to green@10% — freeze-frame bait.

**Overlays**: thread chip: **`EXP-142 · 5 FILES +120 −34`**.
**Sound-off** ✅ — green/red tints + rising counters read as "real work happened" at thumbnail size.
**Build**: `DiffSideBySide` — port marketing diff renderer (18px rows, regex syntax tinting kw #60a5fa / str #4ade80 / num #facc15). HARD: none; the cascade is a per-row `delay = index` interpolate.

---

### S6 — TYPE: WATCH IT. STEER IT. — f390–434 (45f, beats 27–29)

**Overlay (verbatim)** — one centered line, Inter **800**, **110px**:
> **WATCH IT. STEER IT.**

- f390 (b27): "WATCH IT." slams. f405 (b28): "STEER IT." slams.
- The second sentence's period is a **pulsing emerald dot** (`#34d399`, cap-height circle): pulses
  scale 1.0→1.35→1.0 + glow at f405 and f420 (2-beat pulse) — it visually hands off to the emerald
  "Coding now" dot in S7.

**Sound-off** ✅. **Build**: `TypeSlamLine` + dot variant. Free.

---

### S7 — WEB: live steer (the lived-system scene) — f435–539 (105f, beats 30–36)

**Surface**: Web issue detail for EXP-142 **with the agent-session panel** — one build, one crop
(this merges the old "web timeline" beat into the steer beat; the web chrome itself is the receipt).
Design 1440×1000, web tokens at the 18.5px-root scale (rows 46px, text-sm 16.2px). Visible top→bottom
in crop:
1. **Breadcrumb bar**: `● Exponential › EXP-142 › Reconnect live steer sessions after network drop`
   (project dot #6366f1, mono identifier, muted chevrons) — right cluster cropped out.
2. **Segmented control**: `Details` `Changes` pills in bg-muted/50 — Changes active, with the
   **emerald-500 dot + emerald-400 ping** animation (ping = scale 1→2 opacity .6→0 over 30f, frame-derived loop).
3. **"Coding now" badge row**: outline rounded-full badge, `border-emerald-500/40 text-emerald-400`,
   2px emerald dot with ping + text **`Coding now`**; beside it muted `Dennis · MacBook Pro`.
4. **The h-96 panel** (`rounded-md border bg-card/40`): activity feed, presence strip
   (`👁 Dennis (steering)`), pinned **`Latest changes`** strip with mono `+120` emerald / `-34` rose,
   steer composer (`Message the agent…` textarea, mono `Esc` button, ArrowUp send).

**Camera**: f435: scale **1.50**, center (720, 470); slow push →1.56 over the scene (LINEAR),
center drifts to (720, 540) so the composer owns the lower third by the send beat.

**Micro-animations**:
- f435 (b30): badge dot pulses on the downbeat; thread chip in: **`EXP-142 · WEB · LIVE`**.
- f450 (b31): **narration bubble** (Sparkles glyph 60% muted, bubble bg-muted/30) streams
  **word-by-word at 2 frames/word**: `Reconnect loop is in. Refreshing the steer ticket and resubscribing the stream.` (12 words, done f474).
- f480 (b33): **tool row** slides in (24px rise + fade, 8f): 🔧 `Bash` (xs medium) + mono detail `bun run test:steer-relay`.
- f495 (b34): composer hint shows `You're steering`; typing starts: **`add jitter to the backoff`** at **1 char/frame** (25 chars, done f520). Caret = 1px foreground bar, blink 16f cycle.
- **f525 (b36): send** — ArrowUp button flashes indigo `#4f46e5` (2f) + SPRING_POP; the message chips up into the feed as a sent row.
- f532: reply bubble begins streaming `Adding ±20% jitter…` — cut lands mid-stream at f539 (deliberate: the system is alive at the cut).

**Overlays**: thread chip only.
**Sound-off** ✅ — pulsing emerald badge = "live"; a human typing INTO an agent feed and the agent answering = "steer", no words needed. The breadcrumb keeps EXP-142 on screen the whole time.
**Build**: `WebIssueDetailChrome` (breadcrumb + tabs only — thin), `AgentSessionPanel` (narration bubble, tool row, presence strip, composer). This is the biggest fresh build; it's also the product's hero surface — budget it first. HARD: word-streaming layout (pre-measure words, reveal by count — no reflow surprises since the bubble has fixed max-width).

---

### S8 — TYPE: SYNCED. EVERYWHERE. INSTANTLY. — f540–584 (45f, beats 37–39)

**Overlay (verbatim)** — three word-slams, escalating size, Inter **800**, centered single words stacked replace-in-place:
> **SYNCED.** *(f540, 84px)* → **EVERYWHERE.** *(f555, 96px)* → **INSTANTLY.** *(f570, 110px)*

- Each slams scale 1.35→1.0/9f; previous word exits with 4f fade + 12px rise.
- f570–584: micro camera push 1.00→1.05 on "INSTANTLY." (LINEAR).

**Sound-off** ✅. **Build**: `TypeSlamLine`. Free.

---

### S9 — SPLIT: two-way sync, web ⇄ phone — f585–704 (120f, beats 40–47)

**Surface**: split composition on `Background`. **Left 60%** (1152px): web project board pane only
(no sidebar — crop rule), design 1200×1000 at scale **1.15** (rows ≈53px on screen). **Right 40%**:
minimal dark phone silhouette (rounded-rect r=48, 1px white@10 border, no notch detail), mobile glass
issue list inside (zinc gradient, 42px glass rows), floating capsule bottom nav visible with the
**Agents tab's 8px green dot pulsing** (16f cycle). 2px `#262626` divider between panes.

Both panes show the SAME data (fixture §2), tab **`All Issues`** active on web. Groups visible:
- **In Progress** (yellow@10 header): `EXP-142 Reconnect live steer sessions after network drop` (DS, 📅 Jul 15), `EXP-138 Add effort picker to the Start-coding dialog` (DS, 📅 Jul 18)
- **Todo**: `EXP-136 Release progress bar clips on narrow sidebars`, `EXP-146 Widget: retry queue for offline submissions`
- **Done** (green@10 header): `EXP-139 …`, `EXP-131 …` (counts: web `Done 2`, ticking to 4)
(Mobile casing: "In Progress" iOS style, no row chevrons, swipe actions = iOS.)

**Camera**: panes are static; entrance only — left pane slides in from −40px, right from +40px, 10f EASE at f585.

**Micro-animations** (the passive pane updates **+3 frames** after the active one — a visible cause→effect lag that sells sync better than same-frame):
- f591: thread chip: **`ONE WORKSPACE · WEB ⇄ PHONE`**.
- f600 (b41): web cursor clicks **EXP-138's status glyph** → dropdown pops (6f, popover #171717): rows Backlog/Todo/In Progress/**Done**…
- **f615 (b42)**: click "Done". Web: glyph flips to green CircleCheck, row animates down under the Done header over 9f EASE; group counts tick.
- **f618**: phone EXP-138 row flips + slides under its Done header (spring settle); **2-frame emerald hairline flash** runs down the divider f618–620.
- f645 (b44): phone **touch-drag** (28px touch dot at 40% white) on **EXP-136** — row translates −55px over 12f revealing the green **Done** action (`checkmark.circle.fill` on #22C55E).
- **f660 (b45)**: release — spring physics (SPRING_POP on row x), glyph flips to filled green check-circle, row slides down under Done.
- **f663**: web EXP-136 row re-sorts under Done (+3f echo); divider emerald flash f663–665.
- f675–704: settle hold; Done counts read 4 on both panes; EXP-142 alone remains under In Progress — staging S11.

**Overlays**: thread chip only.
**Sound-off** ✅ — mirrored row movement across a split + the hairline flash IS the sentence "synced instantly", both directions; no caption needed beyond S8's claim.
**Build**: `WebBoard` (rows + tinted sticky group headers + status dropdown — reused in S14), `MobileGlassList` + `PhoneFrame` + `CapsuleNav` + swipe action. HARD: the re-sort choreography — implement rows as absolutely-positioned with y = f(groupOrder, indexInGroup) interpolated on status change; both panes derive from the same fixture state timeline so they can never disagree.

---

### S10 — TYPE: REVIEW. MERGE. SHIP. — f705–749 (45f, beats 48–50)

**Overlay (verbatim)** — one line, three beat-slams, Inter **800**, **110px**:
> **REVIEW.** *(f705)* **MERGE.** *(f720)* **SHIP.** *(f735 — "SHIP." in `#6366f1` with a 1-frame glow bloom at f736)*

**Sound-off** ✅. **Build**: `TypeSlamLine`. Free.

---

### S11 — DESKTOP: two-click merge — f750–809 (60f, beats 51–54)

**Surface**: Desktop **Reviews tool window**, the 260px panel cropped at **2.6×** (renders ≈676px wide).
Group header: 8px `#6366f1` dot + `Exponential` (12px semibold muted). ONE row (single-focal-point rule):
```
⑂ EXP-142  Reconnect live steer sessions after network drop     [Merge]
   #214 · exp/EXP-142
```
(`⑂` = git-pull-request icon in green #22C55E; identifier mono muted; Merge = outline xsmall button.)

**Camera**: static, scale 2.6, centered on the row. Cursor enters f753.

**Micro-animations**:
- f756: thread chip: **`PR #214 · REVIEW`**.
- **f765 (b52): click "Merge"** → button morphs to **`Confirm merge`** (danger red text/border) over 5f — the real two-click pattern, shown honestly.
- **f780 (b53): second click** → `Merging…` + 12px spinner (8f).
- **f795 (b54)**: row collapses 4px, **`Merged ✓`** fades in, merged-blue **#60a5fa**, then the whole row eases to 60% opacity. (Fixture: EXP-142 flips `done` here; release progress becomes 8/8.)

**Sound-off** ✅ — Merge → Confirm merge → Merged ✓ is a self-narrating three-state button.
**Build**: `ReviewsPanel` — tiny; shares tool-window chrome (30px header strip) with S12. HARD: none.

---

### S12 — DESKTOP: release ships — f810–869 (60f, beats 55–58)

**Surface**: Desktop **Releases detail** panel, cropped 2.4×. Content (verbatim strings):
- Header row: `‹` back chevron + `v0.12` (12px medium).
- Summary block: 🚀 **`v0.12 — Live steer hardening`** (13px semibold) · meta chips: `📅 Target Jul 15`.
- **Progress bar**: 4px rounded-full track muted@20, green `#22C55E` fill, right label `7 of 8 done` (12px muted).
- Below (adds "one connected system" texture): two issue rows of the release list — `✓ EXP-139 …` done, and **`⏱ EXP-142 Reconnect live steer sessions…`** — which flips to `✓` green at f822 (the S11 merge echoing in, +3f-style).

**Camera**: static scale 2.4 centered on the summary block; micro push →2.48 over f840–869 LINEAR.

**Micro-animations**:
- f813: thread chip: **`RELEASE v0.12`**.
- f822: EXP-142 row's status glyph flips ⏱→✓ (SPRING_POP) — cause.
- **f825–845: progress bar eases 7/8 → 8/8** (20f, EASE); counter ticks to **`8 of 8 done`** at f838 — effect.
- **f855 (b58)**: full-round **`Shipped`** pill pops (1px green@40 border, green text, SPRING_POP) with a **single 1-frame white flash** at f856. Clean — no confetti.

**Sound-off** ✅ — a bar filling to 100% + a green "Shipped" pill is universal.
**Build**: `ReleasesDetail` — small; reuses tool-window chrome + issue-row core from S2's shell. HARD: none.

---

### S13 — TYPE: the tagline, split — f870–914 (45f, beats 59–61)

**Overlay (verbatim)** — two hard slams, Inter **800**, **96px**, two stacked lines:
> **FEEDBACK IN.** *(f870, white)*
> **PULL REQUESTS OUT.** *(f885, "OUT." tinted `#6366f1`)*

Hard cut out at f915 — no fade (per concept).
**Sound-off** ✅. **Build**: `TypeSlamLine`. Free.

---

### S14 — WIDGET → ISSUE: the loop closes — f915–1019 (105f, beats 62–68)

**Part A — f915–959 (3 beats): the widget on someone else's site.**
**Surface**: the 264px `Send feedback` dialog mock (port of `apps/marketing/src/loop` widget mock)
scaled **2.4×** (≈634px), centered, floating over a **dimmed generic webpage wireframe** (gray blocks
at 20% opacity on #0a0a0a — sells "your user's app", not ours). Dialog: title `Send feedback`,
annotated screenshot thumbnail, email field **`sam@userland.dev`**, comment field
**`Onboarding stalls on the repo picker`** (pre-filled, no typing — saves frames), primary `Send`
button (#e5e5e5 on #171717).
- f918: thread chip: **`ANY WEBSITE · FEEDBACK WIDGET`**.
- f921–929: the **red annotation rectangle** (`#EF4444`, 3px) draws clockwise around a region of the screenshot over 8f.
- **f945 (b64): cursor clicks `Send`** — pressed state + ripple on the beat.

**HARD CUT at f960 (b65).**

**Part B — f960–1019 (4 beats): the issue lands on the board.**
**Surface**: `WebBoard` (same component as S9), cropped 1.3× with the **Backlog** group at top of
frame. Post-S11 state (continuity: NO In Progress group — it's empty): `Backlog 1` (EXP-129),
`Todo 1` (EXP-146), Done below.
- **f963**: new row **`EXP-145  Feedback: onboarding stalls on the repo picker`** drops into Backlog —
  slides down 24px + fades in over 8f EASE, with an **indigo highlight sweep**
  (`rgba(99,102,241,0.15)` bg) that decays to the normal row treatment over f963–1005 (LINEAR).
  Backlog count ticks **1→2**; row shows `−` no-priority glyph, `feedback` label chip (#f97316 dot), dashed unassigned avatar.
- f968: thread chip: **`EXP-145 · NEW ISSUE`**.
- f1005–1019: settle hold.

**Sound-off** ✅ — red rect + Send, hard cut, a highlighted row appearing on the tracker: the
feedback→issue pipe reads with zero copy. Matched title string across the cut does the narration.
**Build**: `WidgetDialog` (port), wireframe backdrop (free), `WebBoard` reuse. HARD: none.

---

### S15 — OUTRO: the real logo — f1020–1199 (180f, beats 69–80)

**Surface**: brand card on `Background` (both indigo glows on).

**Micro-animations** (14-frame staggers, EASE everything):
- f1020: hard cut in; near-black + glow.
- f1026: **white disc** (the real logo base, viewBox 0 0 100 100, circle r=50, fill #ffffff) scales 0.92→1.0 over 16f, rendered at **150px**.
- f1030 / f1038 / f1046: the **three exponential-growth bezier cut-curves** wipe through the disc
  left→right as mask cutouts, each drawing over 14f — implement as the exact mask paths from the
  brand spec (`M -5.87 62.01 C 39.09 65.44 48.72 28.71 49.03 -6.21`, etc., stroke-width **6**,
  black strokes in a white mask, clipped to the circle) animated via `strokeDasharray`/`strokeDashoffset`.
  ⚠️ Do NOT use the placeholder striped sphere in `apps/video/src/components.tsx`. Unique mask ids per instance.
- f1064: wordmark **`Exponential`** fades + rises 24px (16f) — Inter **700**, **96px**, ls −2px, `#fafafa`; lockup gap 28px right of the mark.
- f1082: tagline settles beneath (14f fade + 12px rise): **`Issue tracking that ships code.`** — Inter **500**, **34px**, `#a1a1a1`.
- f1100: **`exponential.at`** — JetBrains Mono **500**, **24px**, `#737373`, fades in 12f.
- f1120–1199: static hold (~2.7 s) for loop/rewatch; glow keeps drifting so the freeze never looks like a stall. Last frame loops cleanly into S1's black.

**Overlay text (verbatim)**: `Exponential` / `Issue tracking that ships code.` / `exponential.at` — the film ends in the landing page's own voice.
**Sound-off** ✅. **Build**: `RealLogo` (SVG per spec §3 of video-brand), `OutroLockup`. HARD: none — the curve-wipe is standard dash animation.

---

## 4. Build inventory & complexity ledger

| Component | Scenes | Source | Complexity |
|---|---|---|---|
| `TypeSlamLine` + `StrikeDrop` | S1,4,6,8,10,13 (≈27% of runtime) | new | **HIGH (S1 only)** — re-kern spec in S1; the other five cards are config reuse, near-free |
| `Background`, `EASE` | all | `@exp/video` as-is | free |
| `DesktopShell` (topbar/rail/detail-header/dock) | S2,S3 | port `apps/marketing/src/ide` + ide.css | MED — port, update fixtures (EXP-142, `main`, English dates), only visible slices pixel-true |
| `ScriptedTerminal` + `FrameTyper` | S2,S3 | port marketing script engine | LOW — replace timers with frame lookup |
| `DiffSideBySide` | S5 | port marketing diff | LOW-MED — counters + cascade added |
| `WebIssueDetailChrome` + `AgentSessionPanel` | S7 | **fresh** | **HIGH** — biggest new build; breadcrumb/tabs are thin, the panel (bubbles, tool rows, presence, composer) is the work. Budget first. |
| `WebBoard` (rows, tinted headers, dropdown) | S9,S14B | fresh (web tokens, 18.5px-root px baked in) | MED — reused twice; re-sort choreography is the tricky bit |
| `PhoneFrame` + `MobileGlassList` + `CapsuleNav` + swipe | S9 | fresh (mobile glass spec) | MED — one screen, one interaction |
| `ReviewsPanel`, `ReleasesDetail` | S11,S12 | fresh, shared tool-window chrome + desktop tokens | LOW |
| `WidgetDialog` + wireframe backdrop | S14A | port marketing loop mock | LOW |
| `RealLogo` + `OutroLockup` | S15 | SVG paths from video-brand spec §3 | LOW |
| `Cursor` / touch-dot, `ThreadChip` | all UI scenes | new | LOW |

Fresh-build surfaces: **9** (desktop shell, terminal, diff, web detail chrome+steer panel, web board,
mobile list, reviews, releases, widget) — down from the concept's 12; three are direct marketing ports.

---

## 5. Sound-off readability check (beat-by-beat, muted autoplay)

- **Claims**: every positioning claim lives on a full-screen type card (S1,4,6,8,10,13) — six cards, all Inter 800 ≥84px, white on near-black. Nothing narrative is spoken-only; there is no VO dependency.
- **Proofs**: every UI scene has exactly ONE focal micro-action landing on a beat, color-coded semantically: green play → expanding dock (S2), green `✓ Opened pull request #214` + exit-0 badge (S3), green/red tint cascade + rising `+120 −34` (S5), pulsing emerald `Coding now` + human typing into the agent feed (S7), mirrored row-flips bridged by an emerald divider flash (S9), Merge→Confirm merge→`Merged ✓` blue (S11), bar filling to `8 of 8 done` + green `Shipped` pill (S12), red annotation rect → indigo-highlighted new row (S14).
- **Thread**: the mono thread chip names surface + subject on every UI scene, so a viewer who misses a type card still tracks that ONE issue (EXP-142 → PR #214 → v0.12 → EXP-145) is crossing devices. The chip is the sound-off connective tissue the judges said the 2s vignettes lacked.
- **Rhythm without audio**: all 80 beats are code-enforced (`BEAT=15`); cuts, slams, clicks, pops and flips are the percussion. Verified: no scene relies on text smaller than ~14px effective screen size for its story beat (terminal 27px effective, diff 31px, web rows 53px, mobile rows 42px inside a 1.35× pane).
- **Thumbnail/freeze test**: freeze at any type card = a legible claim; freeze at S3 f240+, S7, S9, S12 = green payoff states that read at 200px wide.

## 6. Cutdown note (0–15 s feed edit, no re-animation)

S1–S4 (f0–299 = 10.0 s: hook → Start coding → terminal → claim) + S15 trimmed to 5 s
(logo resolve + tagline, skip the long hold) = a standalone 15 s cut on the same beat grid.
