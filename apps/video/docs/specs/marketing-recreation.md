# Marketing-site UI recreations — visual archaeology spec

Source of truth for rebuilding the Exponential marketing site's product recreations
(desktop IDE, mobile app, loop diagram, widget mock) pixel-perfectly in React/Remotion,
and for auditing them against the real apps.

Audited files (repo `/home/niach/Projects/2026/exponential`, branch `master`, 2026-07-12):

- `apps/marketing/src/HomePage.tsx` — page composition + hero copy
- `apps/marketing/src/ide/` — `Ide.tsx`, `state.tsx`, `data.ts`, `Topbar.tsx`, `Rail.tsx`, `Sidebar.tsx`, `Board.tsx`, `IssueDetail.tsx`, `Files.tsx`, `SourceControl.tsx`, `Diff.tsx`, `Terminal.tsx`, `bits.tsx`, `icons.tsx`, `syntax.tsx`
- `apps/marketing/src/mobile/MobileDemo.tsx` + `mobile/data.ts`
- `apps/marketing/src/loop/LoopCircle.tsx` + `loop/WidgetPreview.tsx`
- CSS: `styles/ide.css`, `styles/mobile.css`, `styles/loop.css`, `styles/tokens.css`, `styles/site.css`
- `lib/animations.ts` (motion variants)

Other consumers: `DocsPage.tsx` embeds `<IdeDemo view="board|source-control|issue" interactive={false}/>` and `<MobileDemo/>` (static).

---

## 1. How the recreations are built

- **Plain React 19 + Vite, plain handwritten CSS.** No Tailwind, no CSS-in-JS, no shadcn.
  Class names are prefixed by surface: `ide-*` (desktop IDE), `mob-*` (mobile), `loop-*`
  (loop diagram), `wmock-*` (widget mock). Site chrome uses CSS custom properties from
  `tokens.css`; **the product recreations hardcode hex colors on purpose** (they mirror the
  product's forced-dark palette from `packages/design-tokens/tokens.json`, not the site theme).
- **Icons:** `lucide-react`, wrapped so every icon renders with `strokeWidth={1.6}`.
  IDE default size 14; site/mobile default 16. A few custom SVG icons exist in
  `components/icons.tsx` (`IcCircle`, `IcCircleDashed` with `strokeDasharray="2 3"`, `ExpLogo`).
- **Fonts:**
  - Site chrome: `"Geist", ui-sans-serif, system-ui` / `"Geist Mono"` (letter-spacing `-0.01em`,
    font-feature-settings `"ss01","ss02","cv11"`).
  - Product recreations: `"Inter", ui-sans-serif, system-ui, -apple-system, "Segoe UI"` and
    `"JetBrains Mono", ui-monospace, "SF Mono", Menlo, Consolas, monospace` (`--ide-mono`).
- **State:** one React context per demo (`IdeContext` / local state in `MobileDemo`).
  All data is static fixtures in `ide/data.ts` and `mobile/data.ts` — fully deterministic.
- **Interactivity flag:** `IdeDemo({ view, interactive })`. When `interactive=false` the root
  gets `.is-static` (no pointer cursors), all onClick handlers are `undefined`, and the demo is a
  static screenshot-like frame. `view` presets: `board` (default, empty center), `issue`
  (EXP-8 open), `files` (package.json open), `source-control` (SC tab open).
- **Scaling:** the IDE is authored at a fixed **960×640** (`BASE_W=960`, `IDE_H=640`) and shrunk
  responsively via `ResizeObserver` → `transform: scale(w/960)` with `transform-origin: top left`;
  wrapper height = `round(640*scale)`. For Remotion: drop the observer, render at 960×640 and
  scale the composition instead.
- **Animation approach:**
  - Page-level: `motion/react` (Framer Motion) variants — `heroStagger` (staggerChildren 0.1,
    delayChildren 0.1), `heroChild` (y:16→0, 0.5s easeOut), `sectionReveal` (y:20→0, 0.55s
    easeOut, whileInView once, amount 0.15), IDE wrapper entrance (opacity 0, y:24 → 0.6s
    easeOut, delay 0.3).
  - IDE self-play: **`setTimeout` chains** (see §5.9 for the exact script + timings).
  - Mobile self-play: **`setInterval` auto-tour** every 4200 ms (see §6.6).
  - CSS keyframes: caret blink `1.1s steps(2,start) infinite` (`opacity 0` at 50%); pulse
    `1.6s ease-in-out infinite` (`opacity .4` at 50%, mobile `.35`); loop-orbit spin `22s linear`.
  - All motion is gated behind `prefers-reduced-motion`.
- **Remotion caveat:** all timers (`setTimeout`/`setInterval`) and `whileInView` must be replaced
  with `useCurrentFrame()`-driven state; everything else (markup, CSS, fixtures) ports 1:1.

---

## 2. Core design tokens (product recreation palette)

### Surfaces & text (zinc dark)

| Token | Hex | Usage |
|---|---|---|
| bg | `#0a0a0a` | app/window/center/list backgrounds |
| bg-elev | `#171717` | topbar, rail, sidebar, tabbars, dock strip |
| bg-soft / hover | `#262626` | hovers, active tab bg, avatars, inline code, chips |
| btn-plain hover | `#303030` | small plain button hover |
| fg | `#fafafa` | primary text |
| fg-muted | `#a1a1a1` | secondary text, icons |
| fg-dim | `#737373` | tertiary/placeholder/timestamps |
| border | `rgba(255,255,255,0.10)` | all major dividers |
| border-faint | `rgba(255,255,255,0.05)` | row separators (inbox/reviews/group) |
| row-hairline | `rgba(255,255,255,0.03)` | issue-row separators |
| input border | `rgba(255,255,255,0.15)` | inputs/textareas (focus → `#737373`) |
| row hover | `rgba(38,38,38,0.3)` | list-row hover wash |
| row open/selected | `rgba(38,38,38,0.6)` | open issue row / selected tree row |

### Accents

| Token | Hex | Usage |
|---|---|---|
| accent indigo | `#4f46e5` | project dot, rail active icon + 2px accent bar, rail badge, inbox unread dot, "New Issue" button (hover `#4338ca`) |
| primary button | bg `#e5e5e5`, text `#171717` | "Commit & Push", send button, checked checkbox (hover `#fafafa`) |
| green | `#22c55e` | done status, `✓` terminal lines, PR-open icon, coding pill, exit badge, dock tab `✳` star, `feedback` label dot |
| green (mobile live) | `#34d399` | agent dot, Live pill, "Take control" (text on it `#052e1c`) |
| yellow | `#facc15` | in_progress (Timer), medium priority, git-M letter, event dot, syntax number |
| orange | `#f97316` | high priority, Claude `●` terminal bullet |
| red | `#ef4444` | urgent priority, git-D, widget annotation rect |
| danger | `#ff6467` | Stop button icon |
| blue | `#3b82f6` | low priority, git-R, selection `rgba(59,130,246,.3)` |
| light blue | `#60a5fa` | merged tag/icon, syntax keyword |
| hunk text | `#93c5fd` on `rgba(59,130,246,0.05)` | diff hunk header |
| syntax string | `#4ade80` | tinted strings |
| syntax comment | `#737373` | comments |
| mobile indigo dot | `#6366f1` | inbox unread dots (mobile) |

### Status-group header tints (verbatim product)

- Todo `rgba(212,212,216,0.08)` · In Progress `rgba(234,179,8,0.10)` ·
  Done `rgba(34,197,94,0.10)` · Backlog `rgba(113,113,122,0.08)`

### Diff tints

- add `rgba(34,197,94,0.10)` · del `rgba(239,68,68,0.10)` · fill (no counterpart)
  `rgba(38,38,38,0.30)` · file header bg `rgba(38,38,38,0.3)`

### iOS-flavored mobile grays

`#050505` phone frame & steer terminal bg · `#161616` list-row cards ·
`#1c1c1e` chips/search/back-button/keys/fields · `#2c2c2e` avatar/FAB/inbox badge ·
`#3a3a3c` active chip · `#8e8e93` iOS secondary text · `#5b5b60` disclosure chevrons ·
`#d4d4d8` body text on mobile.

### Radii & rhythm

- IDE: buttons/tabs/inputs **6px**; tab-close & branch-tag 4px; pills/avatars/dots `999px`;
  window frame **10px** (1px `rgba(255,255,255,0.1)` border).
- Mobile: phone frame **44px**, screen **36px**, cards/rows **14px**, keys 8px, everything else pill.
- Base font: IDE root **13px / 1.45**; metadata 11–12px; mono ids 11px; terminal 13px mono.
- Row heights: issue rows 28px, tree rows 24px, tabs 24px in 30px bars, dock tabs 22px in a
  29px bar, diff lines 18px, code lines 20px.
- Site radii tokens: `--radius-sm:6 --radius-md:8 --radius:10 --radius-xl:14 --radius-lg:16`.

---

## 3. Home page composition (context the recreations sit in)

Order: `SiteHeader` (sticky 56px, blur) → **Hero** → **Loop section** → **Mobile section** →
`FooterCTA` → `SiteFooter`. Shell max-width 1240px, 32px side padding. Sections pad `80px 0`.

Exact hero copy:

- H1: `Issue tracking that ships code.` (clamp(44px,6vw,80px), weight 600, ls -0.04em)
- Sub: `An issue tracker with a built-in coding IDE. Feedback in, pull requests out.` (18px, #a1a1a1)
- Buttons: `Get started free →` (light primary) · ` GitHub` (ghost)
- Link: `Download the desktop app →` (13px dim)
- Under the IDE: caption `This is the desktop IDE. Go ahead — click around.` (13px, #737373)
- Loop section eyebrow: `The loop` (Geist Mono 11px uppercase, ls 0.12em)
- Mobile section: eyebrow `Mobile`, title `Steer your agents from your pocket.`, sub
  `A coding session is running on your desk — watch its live terminal, take control, and type, from wherever you are. Native apps for iOS and Android.`,
  ghost button `Get the apps →`.
- IDE wrapper shadow: `0 24px 64px rgba(0,0,0,0.5)`.

---

## 4. Desktop IDE recreation — layout skeleton (960×640)

```
┌────────────────────────────────────────────────────────────────────────── 960px ┐
│ TOPBAR 38px  #171717  [● Exponential ⇕]      …      [Dev Server ▾][▶] │ [⎇ master ↑n][✓][↙][↗] │
├──────┬───────────────────────────────────────────────────────────────────────────┤
│ RAIL │ MAIN (column)                                                             │
│ 44px │ ┌───────────────┬─────────────────────────────────────────────────────┐   │
│#17..│ │ SIDEBAR       │ CENTER  #0a0a0a                                      │   │
│  🔍  │ │ 260px (tools) │ ┌ tabbar 30px #171717: [EXP-8 ×][package.json ×]… ┐  │   │
│ ──── │ │ 340px (issue  │ │ tabpane: IssueDetail | FileTab | ScTab          │  │   │
│  📥3 │ │  lists) #17.. │ └─────────────────────────────────────────────────┘  │   │
│  👤  │ │               │                                                      │   │
│  ☰   │ └───────────────┴──────────────────────────────────────────────────────┘  │
│  ⇅2  │ ┌ TERMINAL DOCK (spans sidebar+center, NOT the rail) ────────────────┐    │
│ ──── │ │ collapsed: 29px strip "Terminal (1)"  ·  open: 208px               │    │
│  📁  │ └────────────────────────────────────────────────────────────────────┘    │
│  ⑂   │                                                                           │
│  …   │                                                                           │
│ ⚙ 👤 │                                                                           │
└──────┴───────────────────────────────────────────────────────────────────── 640px┘
```

Window: `border:1px solid rgba(255,255,255,.1); border-radius:10px; background:#0a0a0a; overflow:hidden`.

### 4.1 Topbar (38px, `#171717`, bottom border)

Left → right, 6px gaps, 8px side padding:

1. Project pill (26px h, radius 6, hover #262626): 12px round dot `#4f46e5` + `Exponential`
   (13px, weight 500) + ChevronsUpDown 12 muted.
2. Spacer.
3. Run cluster: ghost button `Dev Server ▾` (12px) · icon button Play 14 in green `#22c55e`.
4. Vertical divider 1×16 `rgba(255,255,255,.1)`.
5. Git cluster (8px gaps): branch button `⎇ master` (12px, #fafafa; the `⎇` is a text glyph) ·
   when ahead: `↑1` (JetBrains Mono 11px #a1a1a1) · icon buttons Check / ArrowDownLeft /
   ArrowUpRight (titles "Commit…", "Pull", "Push").

Ghost button recipe (`.ide-ghost`): inline-flex, 24px h, pad 0 6px, radius 6, 12px text,
`#a1a1a1` → hover `#262626` bg + `#fafafa`. Icon-only variant 24×24.

### 4.2 Icon rail (44px wide, `#171717`, right border)

32×32 buttons, radius 6, icons 16px, gap 4, column pad 8px 0. Active state: icon `#4f46e5` +
2px×(h-4) rounded accent bar at `left:-6px`. Badge: absolute top-2 right-2, min-w 13px, h 13px,
pill, bg `#4f46e5`, white 9px weight-600 text.

Order (top→bottom): Search · ─divider (24×1)─ · **Inbox** (badge = unread count, starts 3) ·
**My Issues** (CircleUser) · **All Issues** (ListTodo) · **Reviews** (GitPullRequest, badge =
open PRs, starts 2) · ─divider─ · **Files** (Folder) · **Source Control** (GitMerge) ·
─spacer─ · Settings · Account (CircleUser). Search/Settings/Account are inert.

### 4.3 Sidebar (260px; **340px** when tool ∈ {issues, my-issues, reviews, inbox})

`#171717`, right border. Non-board panels start with `.ide-toolhead`: 30px bar, pad 0 12px,
bottom border, 13px muted icon + title 11px weight-500 `rgba(250,250,250,.7)`, optional
trailing control.

#### a) All Issues board panel (tool `issues`)

- Header block pad 0 16px: title row (`All Issues` 13px/500 — filter icon-btn (ListFilter 14) +
  **New Issue** button: 24px h, pad 0 8px, radius 6, bg `#4f46e5`, white 11px/500, Plus 12).
- Filter pills row: `All Issues` `Active` `Backlog` — 22px h, pad 0 10px, pill radius,
  12px `#a1a1a1`; active = bg `#262626`, `#fafafa`, weight 500.
- Scrollable list on `#0a0a0a`. Group headers 28px (chev 14 + StatusIcon 14 + label 13px/500 +
  count 11px muted), tinted per status (§2), collapsible. Empty groups hidden.
- **Issue row (28px)**: CSS grid `24px 64px 24px minmax(0,1fr) auto`, pad 0 12px, hairline
  bottom border. Cells: PriorityIcon 14 · identifier (mono 11px `#a1a1a1`) · StatusIcon 14 ·
  title 13px ellipsized (ml 8px) · meta (gap 6): label chips, avatar 16 (initials, bg #262626,
  1px border; empty = 16px dashed-border circle w/ User icon at 55%), due date
  (`CalendarDays 12` + `Jul 9` 11px muted; unset = icon only at opacity .3).
- Status icons: backlog CircleDashed muted · todo Circle `#fafafa` · in_progress Timer
  `#facc15` · done CircleCheck `#22c55e`. Priority icons: none Minus muted · urgent
  TriangleAlert `#ef4444` · high SignalHigh `#f97316` · medium SignalMedium `#facc15` ·
  low SignalLow `#3b82f6`.
- Label chip: 18px pill, pad 0 8px, 1px faint border, 11px muted text, 6px color dot,
  max-w 92px.

#### b) My Issues (tool `my-issues`)

ToolHead `My Issues` (CircleUser). Flat `IssueRow` list of EXP-8, EXP-5 (no groups).

#### c) Inbox (tool `inbox`)

ToolHead `Inbox` + trailing ghost `Mark all read` (visible while any unread). Rows on `#0a0a0a`,
pad 8 12, faint bottom border, hover wash; read rows opacity .6.
Row = 28px round icon badge (bg #262626, muted 13px icon) · main (line1: id mono 11 muted +
title 12px #fafafa, weight 600 when unread; line2 sentence 11px muted, ellipsized) ·
meta (time 10px #737373 + 7px `#4f46e5` unread dot).
Type→icon: issue_assigned UserPlus · issue_comment MessageSquare · issue_status_changed
CircleDot · pr_opened GitPullRequest · pr_merged GitMerge.
Click marks read + opens the issue tab.

#### d) Reviews (tool `reviews`)

ToolHead `Reviews` (GitPullRequest). Project group header: 8px `#4f46e5` dot + `Exponential`
(11px/500 muted). Row: PR icon 14 (open = GitPullRequest green; merged = GitMerge `#60a5fa`) ·
main (id mono 11 + title 12; sub `#214 · exp/EXP-8` mono 10px #737373) · right: **Merge**
button (24px, bg #262626, 1px border rgba(255,255,255,.12)) or `Merged ✓` (11px `#60a5fa`).
Merge → row fades to opacity .45 over .35s, disappears after 900ms; rail badge decrements.
Empty state: `No open pull requests.` (12px #737373). Both fixtures: EXP-8 `#214`, EXP-11 `#209`.

#### e) Files (tool `files`)

ToolHead `Files` (Folder) + trailing Refresh icon-btn. Tree rows 24px, indent `8+depth*14`,
13px text: chevron (dirs) or 14px spacer, Folder/FolderOpen/File icon 14 muted, name, and a
right-aligned git letter (mono 11 bold: M `#facc15` A `#22c55e` D `#ef4444` R `#3b82f6`).
`apps` starts expanded. `steer-terminal.tsx` carries `M`. `Caddyfile` is dimmed (op .55).
Clicking `package.json` opens the read-only code tab.
Tree fixture (root order): `.github/workflows/{build-desktop.yml,build-issues-web.yml}`,
`apps/{android{build.gradle.kts,settings.gradle.kts}, desktop{crates/Cargo.toml,Cargo.toml},
ios{Project.swift,Tuist.swift}, web{src{components{issue-list.tsx,steer-terminal.tsx M},
routes{index.tsx},styles.css},package.json,vite.config.ts}}`,
`packages/{db-schema,design-tokens{tokens.json,package.json},widget}`, `Caddyfile`(dim),
`docker-compose.yaml`, `package.json`, `README.md`.

#### f) Source Control (tool `source-control`)

ToolHead `Source Control` (GitMerge). Section label `Branches` (11px/600 muted).
Branch rows: text glyph `⎇` + name (mono 12; current = #fafafa/500, else muted) + optional
`worktree` tag (10px, 1px border, radius 4) + trailing Check 14 on current.
Fixtures: `master` (current) and `exp/EXP-8` (worktree). Clicking sets "viewed" tint.

### 4.4 Center tab bar & panes

Tab bar 30px `#171717`: tabs 24px h, radius 6, pad `0 4px 0 8px`, 12px text muted; active
bg `#262626` #fafafa. Issue tabs render their label (`EXP-8`) in mono 11px. Each tab has a
16×16 close X (11px icon). Empty state (view `board`): centered Inbox icon 24 muted,
`Nothing open` 13px/500, `Pick an issue from the sidebar — it opens as a tab here.` 11px muted.
Tab kinds: `issue:<id>`, `file:<path>` (label = basename), `sc` (label `Source Control`).

### 4.5 Issue detail tab

```
┌ head (pad 8 16, bottom border): EXP-8(mono 11 muted) … [◉ Coding…][Stop|▶ Start coding][🔔 Subscribed]
├ tabs row (pad 4 16, bottom border): Details · Changes     (plain 12px text, active #fafafa/500)
├ body (flex) ───────────────────────────────────────┬ PROPS 288px (left border, pad 12) ┐
│ main column max-w 768px centered, pad 0 16 24      │ STATUS    [⏱ In Progress]         │
│  title 16px/600 (pad 12 0 6)                        │ PRIORITY  [ᯤ High]                │
│  markdown toolbar (24px icon btns, bottom border)   │ ASSIGNEE  [(DS) Danny Strähhuber] │
│  description 13px/1.6 (2 paragraphs, `pty` inline   │ LABELS    [🏷 Add label]          │
│    code: mono 12, bg #262626, radius 4, pad 1 4)    │ DUE DATE  [📅 Jul 9]              │
│  ── attachments row: right-aligned `0 images` 11px  │            ⟳ Add recurrence 11px  │
│  Activity (2) 13px/500                              │ PROJECT   [● Exponential] chip    │
│   comment: avatar 20 + author 12/500 + time 11      │  (labels 11px/500 UPPERCASE,      │
│   event: 6px yellow dot + text 12 muted + `· time`  │   ls .04em, #a1a1a1)              │
│  composer: input 32px (1px border .15, radius 6,    │                                   │
│   placeholder `Leave a reply...`) + 28px send btn   │                                   │
│   (bg #e5e5e5, Send 14, disabled op .5)             │                                   │
└─────────────────────────────────────────────────────┴───────────────────────────────────┘
```

- Header buttons: `Start coding` (Play 14 green) ↔ while coding: pill `Coding…` (20px pill,
  bg `rgba(34,197,94,.15)`, `#22c55e`, 6px pulsing dot) + `Stop` (CircleX 14 `#ff6467`).
  Subscribe toggle: `Subscribed` (Bell) / `Subscribe` (BellOff); EXP-8 starts subscribed.
- Markdown toolbar groups (separated by 1×16 dividers): [H1 H2 H3] [Bold Italic Strikethrough
  Inline code] [Link Blockquote] [Bullet list Numbered list Task list] [Clear formatting]
  [Insert image] — all 24×24 ghost buttons, icons 14, inert.
- EXP-8 description text (verbatim): para 1 `When the steer relay drops a WebSocket
  mid-session, the terminal view goes stale and never recovers. Reconnect with exponential
  backoff and resume the ` + code `pty` + ` stream from the last acked offset.`;
  para 2 `Repro: kill the relay while an agent session is streaming — the xterm freezes until
  a full page reload.` Other issues show placeholder `Add description...` (60% muted).
- Activity fixtures (EXP-8): comment by `Danny Strähhuber` (`DS`, `3 hours ago`):
  `Backoff should cap at 15s — the relay load balancer kills idle sockets after 60s anyway.`
  Event: `Danny Strähhuber changed status to In Progress` `· 2 hours ago`.
  Empty: `No activity yet. Be the first to add a comment.` Submitting the composer appends a
  comment as `Danny Strähhuber · just now`.
- **Changes tab**: `DiffView` if EXP-8 or the issue has finished a coding run, else
  `No changes yet. Start coding to open a PR.` (centered 12px #737373).

### 4.6 Read-only code tab (`package.json`)

Mono 12px, line-height 20px, 40px right-aligned gutter (50% muted), JSON tinting
(keys `#60a5fa`, strings `#4ade80`, numbers/bools `#facc15`). Content = root workspace
package.json fixture (name `exponential`, workspaces, scripts dev/build/typecheck/migrate/test,
`"packageManager": "bun@1.2.19"`).

### 4.7 Source Control center tab

Two columns: left 360px (right border) + diff pane.
Left: optional `Staged (n)` list, `Changes (1)` list — row: 13×13 checkbox (radius 3, border
.25; checked bg `#e5e5e5` w/ dark check) + git letter + path 11px ellipsized. Fixture change:
`apps/web/src/components/steer-terminal.tsx` `M`. Empty: `No local changes`.
Commit box (top+bottom borders, pad 8): 3-row textarea placeholder `Commit message…`, buttons
`Commit` (plain #262626) and `Commit & Push` (primary #e5e5e5/#171717), both 24px/11px,
disabled at opacity .5 until message + changes exist. Committing prepends to History with meta
`niach · gerade eben` and clears changes; plain commit sets topbar `↑1`.
History label + commit rows (subject 12px #fafafa, meta 11px muted) — fixtures:
`feat(desktop): JetBrains-style IDE shell` / `fix(ios): show compose button only inside a project` /
`fix(mobile): Android issue-open crash` / `feat!: masterplan v5 — per-seat billing`, all meta
`niach · vor N Stunden` (**German** — quirk to preserve or fix deliberately). `Load more` ghost.

### 4.8 Side-by-side diff (`DiffView`, shared by SC tab + issue Changes tab)

- File header 26px (bg `rgba(38,38,38,.3)`, mono 12): path
  `apps/web/src/components/steer-terminal.tsx` … `+24` green `-6` red.
- Hunk line: `@@ -42,10 +42,28 @@ export function SteerTerminal({ sessionId }: SteerTerminalProps)`
  — mono 11 `#93c5fd` on `rgba(59,130,246,.05)`.
- Grid `1fr 1fr`; body min-width 840px inside `overflow:auto`. Cell: 18px h, 40px mono-11
  gutter + `white-space:pre` code, TS-tinted. Left cells get a right border. Kinds: `ctx`
  (plain), `del` (red tint, left only), `add` (green tint, right only), missing side = `is-fill`
  gray. Content: the old 6-line WebSocket `useEffect` on the left vs the 22-line
  reconnect-with-backoff version (`retries`, `reconnectTimer`, `connect` callback,
  `Math.min(1_000 * 2 ** retries.current, 15_000)`) on the right; ctx lines 42–43 and the
  closing `return <TerminalView ref={mount} onData={handleInput} />` frame it.

### 4.9 Terminal dock + scripted agent session

- Collapsed: 29px full-width strip (bg #171717, top border): SquareTerminal 14 +
  `Terminal (1)` (or `(2)` once coding) 12px muted … ChevronUp.
- Open: 208px. Tab bar 29px: tabs 22px h/11px — shell tab `✳ ~/E/r/N/exponential`, agent tab
  `✳ agent · EXP-8` (star `#22c55e` 10px; each with a 14×14 X). Right: Plus icon-btn +
  ChevronDown (hide). Agent tab shows a mono exit badge `0` (green tint pill) once ended.
- Shell pane: `❯ ` green prompt + blinking caret (7×14 `#fafafa` block).
- Agent pane: mono 13/1.3, pad 8 12, lines prefixed `$ ` (#a1a1a1) / `✓ ` (#22c55e) /
  `● ` (#f97316); cmd text #fafafa, output #a1a1a1.
- **Script** (per issue, from `codingScriptFor`):
  1. `✓ Created worktree .worktrees/EXP-8 on branch exp/EXP-8`
  2. `✓ Handing EXP-8 to your coding agent`
  3. `● Reading issue EXP-8 — Live-steer terminal reconnect`
  4. `● Plan: reconnect with exponential backoff, resume stream` (EXP-8-specific; other issues:
     `Plan: implement the change, verify, open a PR`)
  5. `● Edited apps/web/src/components/steer-terminal.tsx (+24 -6)`
  6. `$ git push -u origin exp/EXP-8` ← the only typed-char line
  7. `● Opened PR #214 — Live-steer terminal reconnect` (PR# = fixture or `200+issueNumber`)
  8. `✓ Session finished · 1 file changed`
- **Timing:** start 450ms after "Start coding"; `cmd` lines type at **18ms/char**; delay before
  next line: 500ms if next is `cmd`, 550ms if `claude`, else 420ms; 700ms after the last line →
  state `ended`. On end: agent tab gets the `0` badge and a status line appears at the dock
  bottom: 6px green dot + `Process finished with exit code 0` (11px muted, faint top border).
  Reduced motion ⇒ script renders instantly, ends after 500ms.
- "Start coding" also opens the dock, switches to the agent tab, and marks the issue as coded
  (its Changes tab then shows the diff). Auto-scrolls to bottom.

### 4.10 IDE fixture inventory (issues)

| ID | Title | Status | Priority | Assignee | Labels | Due |
|---|---|---|---|---|---|---|
| EXP-8 | Live-steer terminal reconnect | In Progress | High | DS | — | Jul 9 |
| EXP-11 | Issue board keyboard navigation | Todo | Medium | — | — | Jul 15 |
| EXP-12 | Attachment paste uploads | Todo | None | — | — | — |
| EXP-9 | Recurring issues UI polish | Backlog | None | — | — | — |
| EXP-13 | Widget screenshot annotations | Backlog | None | — | `feedback` #22c55e | — |
| EXP-5 | Side-by-side diff view | Done | Medium | DS | — | — |
| EXP-7 | Terminal exit-code badges | Done | Low | — | — | — |

Group order: In Progress → Todo → Backlog → Done. Filters: All = all four; Active =
in_progress+todo; Backlog = backlog. Assignee: `DS` = `Danny Strähhuber`.
Project: `Exponential`, color `#4f46e5`. My Issues = EXP-8, EXP-5.

Inbox fixtures (unread ×3): `pr_opened` EXP-8 `Your coding agent opened pull request #214 for
EXP-8` 2m · `issue_comment` EXP-12 `Danny commented: paste should reuse the drag-drop path`
26m · `issue_assigned` EXP-11 `Danny assigned you EXP-11` 1h · read: `pr_merged` EXP-5
`Danny merged the pull request for EXP-5` 3h · `issue_status_changed` EXP-7
`Danny changed the status to Done` 5h.

---

## 5. Mobile demo (`MobileDemo`)

### 5.1 Phone frame

330px wide, pad 9px, radius 44px, bg `#050505`, 1px border `rgba(255,255,255,.14)`, shadows
`0 30px 70px rgba(0,0,0,.55)` + `0 4px 18px rgba(0,0,0,.4)`. Screen: 660px tall, radius 36px,
bg `#0a0a0a`, with an 84px bottom fade to `rgba(10,10,10,.92)`. Home indicator: 118×4px pill
at 35% white, 7px from bottom. Entrance: whileInView y:28→0 0.6s.

### 5.2 Status bar (46px)

`20:22` (14px/600) left · centered black **dynamic island** 88×25px pill · right Wifi 15
(stroke 2.2) + custom battery SVG 30×13 (rounded rect outline 35% white, white fill body,
`96` in 8px bold dark text, nub).

### 5.3 App shell — 4 tabs + FAB

Floating bottom bar (absolute, 16px sides, 22px up): left **dock** pill
(bg `rgba(28,28,30,.92)`, blur 12, pad 13 18, gap 20, shadow) with 19px icons — ListTodo
(Issues) · Search · Bot (Agents; **green dot `#34d399`**, pulsing) · Inbox (indigo `#6366f1`
unread dot until visited). Active tab = white. Right: 50px round **FAB** (bg `#2c2c2e`,
SquarePen 20). Dock hidden on the steer screen.
Pane switches crossfade (opacity 0, y:8 → 0.28s easeOut).

### 5.4 Screens

- **Issues**: large title `Exponential` 30px/700 (tappable project switcher w/ 24px
  ChevronsUpDown pill — cycles to project `Mobile App`), hairline divider, chip row:
  30px round filter-icon chip + chips `All Issues` `Active` `Backlog` (30px pill, `#1c1c1e`
  → active `#3a3a3c` white). Grouped list identical semantics to desktop; rows are
  **cards**: margin 3 12, pad 11 12, radius 14, bg `#161616` — Priority 15 · id (mono 11.5
  `#8e8e93`) · Status 15 · title 15px · optional label pill/avatar 20 · ChevronRight 15 `#5b5b60`.
- **Search**: title `Search`, live pill field (38px, `#1c1c1e`) showing query `auth` + blinking
  2px caret; section head `Exponential` (13px/600 `#8e8e93`) with 3 results (EXP-3
  `Google auth on the register page` done · EXP-14 `Auth session refresh on wake` in-progress
  high DS · EXP-19 `OAuth error toasts` todo); section `Assigned to you` (EXP-8, EXP-5).
- **Agents**: title `Agents`; rows with pulsing 8px green dot + id + two-line main
  (title 15px / meta 12px `#8e8e93`): EXP-12 `Attachment paste uploads` —
  `Claude on dennis-mbp · 12m`; EXP-8 `Live-steer terminal reconnect` —
  `Claude on dennis-mbp · 34m`. First row opens Steer.
- **Steer (live terminal)**: header = 32px round back button (`#1c1c1e`) · `EXP-12` mono ·
  `● Live` pill (`rgba(52,211,153,.15)` / `#34d399`, 22px) · right presence chips `👁 2` and
  keyboard chip. Terminal card: margin 12, radius 14, bg `#050505`, 1px 8% border, JetBrains
  Mono 11px/1.55. 8 static lines: `✓ Created worktree .worktrees/EXP-12` ·
  `$ claude --dangerously-skip-permissions` · `● Reading issue EXP-12 — Attachment paste
  uploads` · `● Plan: intercept paste, reuse the drop upload path` · `● Edited
  issue-editor/paste-upload.ts (+41 -3)` · `● Running bun run typecheck…` · `✓ typecheck
  passed · 0 errors` · `● Committing and opening the pull request` + blinking caret.
  Bottom input area: key chips `esc ^C tab ↑ ↓` (26px, radius 8, mono 11) + field row:
  `Type to steer…` pill field (36px) + **Take control** pill (bg `#34d399`, text `#052e1c`,
  12.5px/600).
- **Inbox**: title `Inbox`; card rows = 28px round badge (`#2c2c2e`) + id + title (13.5px,
  bold when unread) + sentence 12px + time + 7px `#6366f1` dot. Fixtures: EXP-12
  `Claude opened pull request #217` 2m unread · EXP-8 `Danny commented: backoff should cap at
  15s` 1h unread · EXP-5 `Danny merged the pull request` 3h · EXP-11 `Danny assigned you` 5h.
- **Issue detail** (static variant, `screen="issue"`, used by docs): back header + `EXP-8`,
  title 23px, grouped props card (Status ⏱ In Progress / Priority ᯤ High / Assignee DS
  Danny Strähhuber; 11px pad rows, 6% separators), 2-paragraph description 14.5px `#d4d4d8`,
  Activity: yellow-dot event `Danny changed status to In Progress · 2 hours ago` + comment card
  (`Danny Strähhuber · 1 hour ago`: `Repro: toggle Wi-Fi while a session is streaming. The
  cursor freezes but the session keeps running fine.`).

### 5.5 Second project fixture (`Mobile App`)

MOB-4 `Offline issue drafts` (in-progress, high, DS) · MOB-7 `Push notification deep links`
(todo, medium) · MOB-9 `Haptics on swipe actions` (todo) · MOB-2 `Widget screenshot viewer`
(done, DS).

### 5.6 Auto-tour script (`autoTour`, home page)

`setInterval` 4200 ms through: issues/chip0 → issues/chip1 (Active) → issues/chip2 (Backlog) →
agents → steer → inbox (marks inbox read) → loop. Any pointer-down cancels the tour.
Disabled under reduced motion.

---

## 6. Loop diagram + widget mock (home "The loop" section)

Two-up grid (max-w 860, gap 32): LoopCircle left, WidgetPreview right. These use **site**
tokens (`var(--border)` etc.), not the product palette.

- **LoopCircle**: square `min(320px,72vw)`; SVG ring r=37.6 (viewBox 100), 1px `--border`
  stroke; a 6px `#a1a1a1` dot orbits via a rotating wrapper (22s linear). Five 42px circular
  chips (bg `#171717`, 1px border, muted 16px icons) placed clockwise from 12 o'clock at 72°
  steps, each with name (12px/500) + phrase (11px `#737373`):
  `Feedback / a user reports a bug` (MessageSquare, label above) ·
  `Issue / lands on the board` (CircleDot, right) · `Code / Claude writes the fix` (Terminal,
  right) · `PR / review, merge` (GitPullRequest, left) · `Ship / the reporter hears back`
  (Rocket, left).
- **WidgetPreview** (static mock of the feedback widget): 264px card, radius 12, bg `#171717`,
  shadow `0 16px 48px rgba(0,0,0,.45)`. Head `Send feedback` (12.5px/600) + X 13. Body:
  fake screenshot box 76px (3 gray skeleton bars w60/w80/w40, a 44×22 **red `#ef4444`
  annotation rectangle**, overlay chips `Annotate` `Retake` at 9.5px on `rgba(0,0,0,.65)`);
  field `Title` → `Checkout button does nothing`; field `Details` →
  `Clicked "Pay now" on Safari — no response.`; right-aligned light `Send feedback` button.
  Caption below: `The drop-in feedback widget — screenshot included.`

---

## 7. Remotion reuse assessment

**Directly reusable (near copy-paste):**
- All markup + `ide.css` / `mobile.css` (self-contained, prefix-scoped, hardcoded hex — no
  site-token dependency except loop/wmock). Import the CSS, render `<IdeDemo interactive={false}>`
  inside a fixed 960×640 box (strip `useIdeScale`).
- All fixture data (`ide/data.ts`, `mobile/data.ts`) and the icon wrappers (lucide, stroke 1.6).
- The `bits.tsx` atoms (StatusIcon/PriorityIcon/Avatar/LabelChip) — the product's visual language.

**Must be rewritten for Remotion (frame-driven):**
- IDE coding script: replace the `setTimeout` chain with a pure function
  `scriptPosAtFrame(frame)` reproducing §4.9 timings (18ms/char typing; 500/550/420/700ms gaps;
  450ms lead-in). The render layer (`TermLine`, `slice(0, done)` + partial chars) already takes
  `ScriptPos` as data — keep it, feed it from `useCurrentFrame`.
- Mobile auto-tour: replace `setInterval` with `TOUR[floor(frame/ (4.2s*fps)) % 6]`; pane
  crossfade (opacity/y 0.28s) via `interpolate`.
- CSS keyframe animations (caret blink 1.1s steps(2), pulse 1.6s, orbit 22s) — either keep as
  CSS (Remotion renders them non-deterministically) or re-drive: caret `opacity = frame%(1.1s)
  < .55s ? 1 : 0`, orbit `rotate(frame/22s*360deg)`.
- motion/react entrances (`whileInView`, hero stagger) → `spring()`/`interpolate`.
- State transitions demoable in video: open issue tab, switch filter pills, Start coding →
  dock opens → typed script → exit badge + status line → Changes tab shows diff → Reviews
  Merge → fade-out; SC commit flow (`Commit & Push` clears changes, prepends history).

**Watch-outs:** fonts must be loaded in Remotion (Inter + JetBrains Mono; Geist only for site
chrome); the `⎇` and `✳` and `❯`/`✓`/`●` glyphs are plain text, ensure font fallback renders
them; German commit metas (`vor 3 Stunden`, `gerade eben`) and `niach` author may want
localization for a launch video; `~/E/r/N/exponential` shell tab title is a fish-style
abbreviated path.

---

## 8. Inventory for the staleness diff (recreation vs real apps)

What the recreation **shows** (checklist to verify against the real gpui desktop / native apps):

- Rail tools: Search(inert), Inbox, My Issues, All Issues, Reviews, Files, Source Control,
  Settings(inert), Account(inert). **No Releases surface** — the real product added releases
  (EXP-56) as a rail/sidebar-level surface on all four clients; the recreation predates it.
- Statuses: only backlog/todo/in_progress/done — real enum also has **cancelled/duplicate**.
- Markdown toolbar: H1-H3, bold/italic/strike/code, link, quote, lists incl. task list, clear
  formatting, image — but no **@mention / #issue-mention** affordances (real editors have
  @-autocomplete and #EXP-n pills).
- Issue detail: Details/Changes tabs, Start coding/Stop, Subscribe, props = Status/Priority/
  Assignee/Labels/Due date(+Add recurrence)/Project. No release property, no duplicate-of,
  no archived, no PR chip in props (PR appears only via Reviews/inbox copy).
- Coding flow shown: worktree + `exp/<ID>` branch + agent session + `git push` + PR open —
  matches the v2 launcher contract (one issue = one PR = one branch). No model/effort pickers,
  no release orchestrator, no run-config editor (topbar just shows a static `Dev Server` pill),
  no MCP/.mcp.json detail, no `claude --dangerously-skip-permissions` on desktop (the mobile
  steer script *does* show that command).
- Terminal: simplified 3-glyph line grammar (`$`/`✓`/`●`), not a real PTY; exit-code badge and
  "Process finished with exit code 0" strip mirror the real dock.
- Mobile: 4-tab dock (Issues/Search/Agents/Inbox) + compose FAB + project switcher + live steer
  (presence chips, key row esc/^C/tab/↑/↓, "Take control") — verify against current iOS/Android
  nav (onboarding, releases, feedback boards are absent here).
- Identity fixture: `Danny Strähhuber` / initials `DS` / machine `dennis-mbp` / git author
  `niach`; PR numbers #214/#217/#209; project color `#4f46e5` (indigo).
- Web app surfaces are **not** recreated at all (no browser-frame recreation of the web issue
  tracker, no public feedback board, no admin/billing) — the only web-ish artifact is the
  widget dialog mock.
