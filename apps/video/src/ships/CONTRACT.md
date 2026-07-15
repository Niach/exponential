# ShipsItsOwnIssues — build contract

50s launch film, 1920×1080 @ 30fps, 1500 frames, composition id `ShipsItsOwnIssues`.
Everything is recreated UI (zero screenshots in the render). Storyboard:
`/tmp/claude-1000/-home-niach-Projects-2026-exponential/4bbbcd38-3f18-4a85-b1f0-3f21e631aef4/scratchpad/concepts/board-1-ships-its-own-issues.md`

## Architecture

- `src/ships/theme.ts` — palette `C`, `WIN` metrics, `UI_FONT`/`MONO_FONT` (already loaded), `EASE`, `POP`, `SETTLE`. NEVER hardcode colors/fonts — import from here.
- `src/ships/fixtures.ts` — the ONE fixture world (board rows, hero issue, session scripts, release, diff, lanes, copy). Never invent divergent content.
- `src/ships/rig.tsx` — `Camera`/`camAt`, `WindowChassis`, `CursorLayer`, `Caption`, `Punch`, `ExpLogo`, `WordmarkChip`, `typed`, `riseIn`, `rollNum`, `useBlink`.
- `src/ships/surfaces/*.tsx` — pure presentational surfaces (this phase).
- `src/ships/scenes/` + `src/ships/ShipsItsOwnIssues.tsx` — assembly (later phase; do NOT create).

## Hard rules (all builders)

1. **Frames**: every surface takes a `frame: number` prop = COMPOSITION-GLOBAL frame. Micro-animations take global start frames via props (e.g. `revealAt`, `schedule`). No `useCurrentFrame()` inside surfaces — the assembler passes `frame` down. No timers, no `Math.random()`, no `Date`.
2. **Remotion markup rules**: `interpolate()` inline where practical, always `extrapolateLeft/Right: "clamp"`, easing `EASE` from theme. Use CSS `scale`/`translate`/`rotate` properties (not `transform` strings). CSS transitions/animations are FORBIDDEN. `spring()` from remotion with `POP`/`SETTLE` configs is allowed (pass `fps: 30`, `frame: frame - startFrame`).
3. **Coordinates**: window surfaces lay out in window-local px inside a 1568×980 box (`WIN`). Shell grid: top bar h38 (y 0–38) · icon rail w44 (x 0–44) · tool sidebar w260 (x 44–304) · center x 304–1568 · properties panel w288 (right edge of center when shown) · dock: expanded h240 (y 740–980: tab strip 29px, grid, exit strip), collapsed strip h29 (y 951–980).
4. **Pixel truth**: match the reference screenshots in `apps/video/ref/` (Read them — they are PNGs you can view). Where the storyboard and a screenshot disagree, THE SCREENSHOT WINS (real product). Sizes in refs are ~1999px-wide captures of the real app — treat proportions, not absolute px, as truth; our window is 1568 wide.
5. **Icons**: no icon library — draw tiny inline SVGs (stroke 1.6–2, `currentColor`) mimicking lucide. Keep each icon a small local component. SVG ids must use `useId`.
6. **Text**: UI text `UI_FONT`; identifiers/terminal/code `MONO_FONT`. Desktop UI base sizes: rows 13px, small/meta 11–12px, titles 15–20px (see spec/refs).
7. **TypeScript strict**, no new dependencies, no CSS files. Named exports only. Every file must compile standalone against theme/fixtures/rig (`bun x tsc` in apps/video must stay green).
8. Components must tolerate any frame value (clamp everywhere); render resting state when a scheduled animation hasn't started.

## Surface files & required exports (prop shapes may be extended, not renamed)

### surfaces/chrome.tsx
- `TopBar({ frame })` — 38px: left logo glyph 16px + "Exponential" 13px/600 + globe + switcher carets; right: run select `Dev Server` + green play ▷, divider, git cluster `⎇ main` chip + commit check icon + `↑1` chip. Match ref/desktop-hero-board-issue.png top strip (bg #0a0a0a, hairline bottom border).
- `IconRail({ frame, active })` — 44px vertical: icons search, inbox, agents(person), issues(list, the board), reviews(git-PR), releases(rocket), files(folder), source-control; bottom: settings, account. `active: string` tints icon `C.indigoSoft` + 2px left accent bar. Small amber dot support via `dots?: string[]`.
- `TabsBar({ frame, tabs, activeId })` — center tab strip (29px): tabs `{ id, label, mono?: boolean }`, each with ×; active tab brighter bg + top accent. Include `popAt?: Record<string, number>` for POP-in of a tab.
- `DockCollapsedStrip({ frame })` — 29px bottom strip: `▤ Terminal (1)` + chevron-up right.
- `CenterEmptyState({ frame })` — centered: inbox-ish icon 24px muted, "Nothing open" 13px/500, sub "Pick an issue from the sidebar — it opens as a tab here." 12px muted.

### surfaces/board.tsx
- Primitives (export): `StatusIcon({ status, size })`, `PriorityIcon({ p, size })`, `Avatar({ initials, size })`, `LabelChip({ name, dot })`, `CalendarGlyph`.
- `SidebarPane({ children, title, actions })` — the 260px tool sidebar chassis (bg #171717? NO — match ref: sidebar bg is `#0a0a0a` w/ right hairline; verify against ref shot) with header row (title 13px/600) — check ref for exact look ("All Issues" + Filter ghost + indigo "+ New Issue" xs button) and pill tabs row (`All Issues` active #262626, `Active`, `Backlog`).
- `BoardTool({ frame, rows, overrides, cascadeAt, hover, selectedId, prDotId, regroup })` — grouped rows per fixtures.BOARD order (In Progress → Todo → Backlog → Done). Group header: chevron + status icon + label 12px/500 + count muted, full-width tint band (C.tint*). Row h28, grid: [10px pad | priority 16 | id mono 11px muted 52px | status 16 | title 13px #fafafa truncate | label chip | avatar 18 | calendar]. `overrides: Record<id, Partial<BoardRow>>` reassigns status; `regroup?: { id, t }` FLIP-slides a row between groups (t 0→1); `cascadeAt?: number` staggered entrance; `prDotId?: { id, at }` pops a 6px green dot after the identifier.
- `ReviewsTool({ frame, mergeState, hover, rowFade })` — group header `● Exponential`; one row: PR icon (#22c55e) + `EXP-142` mono + title 13px + sub `#214 · exp/EXP-142` 11px muted; right button morph: `mergeState: "rest" | "confirm" | "merging" | "gone"` (outline xs "Merge" → red-outlined "Confirm merge" → "Merging…" + spinner → row collapse via `rowFade` t).

### surfaces/detail.tsx
- `IssueDetailPane({ frame, tab, codingNow, prChip, staggerAt })` — the center pane for EXP-142 (fixtures.HERO), properties panel included at right (288px). Header row: `Details  Changes` PLAIN TEXT tabs (13px, active #fafafa, inactive muted — see ref, NOT pills) + optional green live dot on Changes; right cluster: `3 / 8` muted + chevron up/down + `▷ Start coding` ghost (13px) + `🔔 Subscribed`. Title 20px/600. Markdown toolbar (H1 H2 H3 | B I S code | link quote | list ol task | Tt | image — 15px muted ghost icons). Body paras 13.5px #d4d4d4. `0 images` meta row. `Activity (2)` rows: icon circle + "**Alex Carter** added label bug" 12.5px. Reply composer `Leave a reply...` + send. Properties panel sections (UPPERCASE 11px muted labels): STATUS (status icon + text), PRIORITY, LABELS, RELEASE (🚀 v0.12), DUE DATE (`📅 Jul 15` + `⟳ Add recurrence`), PROJECT (`● Exponential` secondary pill). NO ASSIGNEE section (matches ref). `codingNow?: { at: number }` springs the "Coding now · MacBook Pro" pill (rounded-full, green dot, 1px #22c55e@40 border) next to Start coding→Stop.
- `staggerAt?: number` — properties stagger-fade in (4f stagger, 8px rise).

### surfaces/terminal.tsx
- `TerminalDock({ frame, height, tabs, activeTab, feed, inputGlow, exitAt })` — dock at window bottom: tab strip 29px (session tabs: status dot + label 12px; `zsh`; `+`), grid area (JetBrains Mono 12px/1.45, bg #0a0a0a), then input box (hairline top/bottom rules, `❯ ` + blinking block cursor) + status footer `▶▶ bypass permissions on (shift+tab to cycle) · esc to interrupt · ⏎ for agents` (bypass part red-ish). Real Claude Code CLI grammar per ref/desktop-claude-session-dock.png:
  - tool event → `● Tool(args)` (dot #22c55e, tool name bold white, args muted) + `  ⎿ result` muted line
  - prose event → `● text…` (white dot, wraps, #d4d4d4)
  - spinner → `✳ Verb… (Xm YYs · ↓ ZZ.Zk tokens)` — ✳+verb #eab308, parens muted; elapsed/token numbers must tick (rollNum) while visible
  - flash → line lands with green flash (bg #22c55e@20 → 0 over 8f)
- `feed: { events: SessionEvent[], schedule: number[] }` — schedule[i] = global frame event i reveals (3f fade + 4px rise; args of tool lines may type via `typed`). Auto-bottom-align like a real terminal (newest above input).
- `exitAt?: number` — exit strip `● Process finished with exit code 0` + green `0` badge POP on the tab.
- `height` animatable (29→240) by assembler.

### surfaces/dialogs.tsx
- `DialogScrim({ frame, in, out })` — black@50 fade.
- `IssueCodingDialog({ frame, appearAt, modelMenu, effortMenu, planCheckAt, buttonState, collapseAt })` — 420px, radius 8, bg #171717, border. Title `Start coding on EXP-142` 15px/600 + × right. Intro 12px muted: "Claude works on EXP-142 in its own worktree and opens the pull request when done." Side-by-side labeled selects Model/Effort (label 12px/500 above, select h30 bordered rounded w/ value + caret). `modelMenu/effortMenu: { openAt, closeAt, options, highlight }` renders the popover menu (bg #171717, border, check on selected). Plan mode checkbox row + 2-line muted caption (copy from ref/desktop-release-detail-dialog.png bottom block). Footer right: ghost Cancel + primary (#e5e5e5/#171717) `Start coding` → `Starting…` + spinner via `buttonState`.
- `ReleaseCodingDialog({ frame, appearAt, checkShimmerAt, ultraPulseAt, buttonState, collapseAt })` — 560px, matches ref/desktop-release-detail-dialog.png EXACTLY: title `Start coding on release`; intro `One Claude orchestrator implements the checked issues of "v0.12" — one subagent per issue.`; repo header `Niach/exponential` 12px/600; checklist rows (checkbox CHECKED for fixtures.RELEASE.dialogIssues, id mono muted + title 13px); Model `Fable` / Effort `CLI default` with caption `ultracode sets effort`; Subagent model `Inherit` / Subagent effort `Inherit`; switch row bold `Dynamic workflows (ultracode)` + caption `Runs the orchestrator with --effort ultracode — works with any model.` + toggle ON (light thumb); `Plan mode` checkbox + caption block; footer left muted `Select at least one issue.` hidden when valid, Cancel + `Start coding`.

### surfaces/releases.tsx
- `ReleasesTool({ frame, hover })` — sidebar list: header `Releases` + plus; one row: rocket 15px + `v0.12` 13px/600 + sub `Target Jul 18 · 3 of 8 done` 11px muted.
- `ReleaseDetailTool({ frame, drillAt, progress, shippedAt, cascadeDoneAt })` — sidebar drill-in (slides left 24px on `drillAt`): back chevron + `v0.12` header; action row `+ Add issues · ▷ Start coding · ⋯` (match ref header layout); meta: `📅 Target Jul 18`; 4px progress bar (green fill, animated via `progress: { at: number, from: number, to: number }[]` steps) + right label `N of 8 done` (rolls); below, member issues grouped by status (checklist look per ref: status icon + id mono + title 12.5px). `shippedAt?: number` → green `Shipped` pill POP next to name + rocket tints green (+ optional `⑂ PR #19 · merged`-style chip per ref — here `PR #219 · merged`).

### surfaces/diffview.tsx
- `ChangesPane({ frame, paintAt, statsRollAt, scrollY })` — center pane: header row `⎇ exp/EXP-142 · PR #214 · 5 files +120 −34` (branch mono #fafafa, +green/−red; digit-roll stats at `statsRollAt`) + right ghost `Open terminal in worktree` + `⋯`; 240px file list (fixtures.DIFF_FILES, status letters M #eab308 / A #22c55e, mono 11px, selected row tint); side-by-side diff of fixtures.DIFF_ROWS: two panes w/ 34px number gutters, hunk header band (C.hunkBg/#60a5fa mono 11px full-width), rows 18px mono 11px, del rows red tint left pane / add rows green tint right pane (hot flash → settle after `paintAt + i`), light TS syntax tint (keywords #60a5fa, strings #4ade80, numbers #facc15) via a tiny regex tokenizer. Match gutter/tint look of ref/desktop-source-control-diff.png.

### surfaces/flowgraph.tsx
- `FlowGraph({ frame, schedule })` — center-pane branch graph per fixtures.LANES, visual language from ref/desktop-source-control-diff.png's left tree (indented rows, vertical connector rails, `worktree` tags, green check state dots) but staged big: 34px rows, mono 16px labels, 1px rails muted@35. `schedule`: `{ drawMain: number, drawRel: number, wave1At: number[], wave1MergeAt: number[], wave2At: number[], wave2MergeAt: number[], prChipAt: number }`. Lane draw = dash-offset reveal; merge = green pulse ring (12px, 8f) at the junction + PR dot yellow→green; `prChipAt` pops chip `⑂ PR #219 · open` on the rel lane.

### surfaces/phone.tsx
- `PhonePiP({ frame, x, y, rotate, feedSchedule, sendPulseAt })` — 330px-wide iPhone chassis (#050505, radius 44, dynamic island, status bar `23:39` + wifi/battery). Screen = the REAL iOS steer activity view per ref/ios-steer-activity.png: header `● Live · Alexs-MacBook-Pro.local` (green dot, 13px muted) + round × button; feed of fixtures.PHONE_FEED — tool rows (crossed-tools glyph 12px + bold name 13px + muted mono summary) and narration bubbles (rounded-16 card, hairline border, sparkle glyph in gutter, 13px text); floating `Jump to latest ↓` pill; pinned `Latest changes  +51 −16 ⌃` strip; composer `Message the agent…` rounded input + circular ↑ send. `feedSchedule: number[]` per item (2f after desktop counterpart — assembler provides), feed auto-scrolls (translate) so newest visible.

## Reference material per builder

- Screenshots: `apps/video/ref/*.png` (READ them).
- Detailed spec: `/tmp/claude-1000/-home-niach-Projects-2026-exponential/4bbbcd38-3f18-4a85-b1f0-3f21e631aef4/scratchpad/specs/desktop-ide.md` (+ mobile.md for the phone).
- Port material (styling/JSX patterns, adapt to our contract): `apps/marketing/src/ide/*.tsx`, `apps/marketing/src/mobile/MobileDemo.tsx`.
