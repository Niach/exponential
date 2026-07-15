# Exponential — Video / Brand Spec (visual archaeology)

Source of truth for recreating the Exponential UI pixel-perfectly in React/Remotion for a
launch video. Everything below was extracted from the real repo at
`/home/niach/Projects/2026/exponential` on 2026-07-12:

- `apps/video/` — existing Remotion package (`@exp/video`)
- `packages/design-tokens/tokens.json` + generated `apps/desktop/crates/theme/src/tokens.generated.rs` (authoritative OKLCH→hex conversions)
- `apps/web/src/styles.css`, `apps/web/src/lib/domain.ts`, real UI components
- `apps/web/public/logo-*.svg`, `apps/marketing/src/components/icons.tsx` (logo geometry)
- `apps/video/public/shots/*.png` — real 2× screenshots of the running app (2880×1800 = 1440×900 CSS px)

---

## 1. Existing Remotion package (`apps/video`, workspace name `@exp/video`)

### Package facts
- Remotion `4.0.484` (exact-pinned), React `19.2.3`, `@remotion/google-fonts` `4.0.484`.
- `remotion.config.ts`: `Config.setVideoImageFormat("jpeg")`, `Config.setOverwriteOutput(true)`.
- Scripts: `studio` (remotion studio), `render` (`remotion render WebUiDemo out/webui-demo.mp4`), `build` (bundle), `lint`.
- Repo-root dispatchers: `bun run studio:video`, `bun run render:video`.
- Pure inline styles, no Tailwind. `src/index.css` is only `* { box-sizing: border-box }`.
- `public/shots/` holds 5 real screenshots: `01-login.png`, `02-board.png`, `03-issue.png`, `04-my-issues.png`, `05-inbox.png` (all 2880×1800, DPR 2 → design at **1440×900 CSS px**).

### Composition (src/Root.tsx)
ONE composition today:

| id | size | fps | duration |
|---|---|---|---|
| `WebUiDemo` | 1920×1080 | 30 | 590 frames ≈ 19.7 s |

### Timeline (src/Video.tsx) — sequences overlap ~8 frames for crossfades
```
frame:   0        72 80        182 190       362 370       504 512      590
         |Intro......|            |             |             |            |
                  |Login..........|
                              |Board.............|
                                            |Issue............|
                                                          |Outro..........|
Intro  from=0   dur=80    logo scale 0.6→1 + wordmark slide-in + tagline fade
Login  from=72  dur=118   ShotScene kicker="Sign in"      title="One clean way in"        width=1180
Board  from=182 dur=188   ShotScene kicker="Issue board"  title="Everything, grouped and live" width=1300
Issue  from=362 dur=150   ShotScene kicker="Issue detail" title="Every detail in one place"    width=1300
Outro  from=504 dur=86    logo+wordmark scale 0.9→1, then "Electric SQL · TanStack Start · React 19"
```
(`04-my-issues.png` and `05-inbox.png` exist but are unused by the current timeline.)

### Reusable components (src/components.tsx)
- **`EASE`** = `Easing.bezier(0.16, 1, 0.3, 1)` — the single easing for everything (expo-out feel).
- **`Background`** — `#09090b` fill + two radial indigo glows:
  `radial-gradient(720px 520px at {50+drift/10}% 32%, rgba(99,102,241,0.20), transparent 70%)` where
  `drift` interpolates 0→40 over frames 0–300, plus a static
  `radial-gradient(600px 400px at 88% 92%, rgba(129,140,248,0.10), transparent 70%)`.
- **`Logo size`** — ⚠️ an APPROXIMATION (striped sphere: light gradient disc `#fafafa→#c7c7d1` with 3 horizontal
  dark bands). It does **not** match the real brand mark. Replace with the real SVG in §3.
- **`BrowserWindow {src,url,width,zoom,frame}`** — browser mockup:
  radius 16, border `1px solid rgba(255,255,255,0.08)`, bg `#18181b`,
  shadow `0 40px 120px rgba(0,0,0,0.6), 0 8px 24px rgba(0,0,0,0.4)`.
  Title bar height = `round(width*0.03)`; three traffic lights `#ff5f57 #febc2e #28c840`
  (diameter `0.28*barH`, opacity .85, gap `0.28*barH`, left pad `0.5*barH`); centered pill URL field
  (`rgba(255,255,255,0.05)` bg, radius=barH, height `0.58*barH`, min-width `0.32*width`,
  fontSize `0.36*barH`, color `#a1a1aa`) showing `app.exponential.at`. Screenshot `<Img>` below at 100% width.
- **`Scene {durationInFrames}`** — opacity envelope `[0,14,dur-12,dur] → [0,1,1,0]` with EASE.
- **`Kicker`** (in Video.tsx) — `#818cf8`, 28px, weight 600, letterSpacing 4, uppercase, fade over 16f.
- **`Headline`** — `#fafafa`, 76px, weight 700, letterSpacing −1.5, fade+rise (16px) over 20f, delay 4.
- **`ShotScene`** — column: Kicker + Headline (gap 14) then BrowserWindow (gap 44, page padding 60);
  window rises 46→0 px and scales 0.97→1 over frames 6–30.
- **IntroScene** — row (gap 28): Logo 118px, wordmark "Exponential" 108px/700/ls −3 `#fafafa`;
  tagline "The real-time issue tracker" 40px/500/ls 0.5 `#a1a1aa` (gap 30).
- **OutroScene** — Logo 78 + wordmark 78px/700/ls −2; line
  "Electric SQL · TanStack Start · React 19" 30px/500/ls 3 `#a1a1aa`.

### Current video theme (src/theme.ts)
```ts
bg        #09090b   // zinc-950 — note: the APP background is #0a0a0a; keep #09090b only for video canvas if desired
bgSoft    #111113
panel     #18181b   // zinc-900
border    rgba(255,255,255,0.08)
text      #fafafa
textMuted #a1a1aa   // zinc-400
accent    #6366f1   // indigo-500
accentSoft #818cf8  // indigo-400
```
Font: `loadFont("normal", { weights: ["500","600","700"], subsets:["latin"] })` from
`@remotion/google-fonts/Inter` — exports `fontFamily`.

---

## 2. Brand palette — canonical tokens (packages/design-tokens/tokens.json)

The product is **forced dark** (`html.dark`), neutral zinc, OKLCH-authored. Exact sRGB conversions below
are from the committed generator output (`tokens.generated.rs`) — use these hexes verbatim.

### Surface palette (OKLCH → hex)
| token | OKLCH | hex / rgba |
|---|---|---|
| background | `oklch(0.145 0 0)` | **#0a0a0a** |
| foreground | `oklch(0.985 0 0)` | **#fafafa** |
| card / popover / sidebar | `oklch(0.205 0 0)` | **#171717** |
| card/popover/sidebar-foreground | `oklch(0.985 0 0)` | #fafafa |
| primary | `oklch(0.922 0 0)` | **#e5e5e5** |
| primary-foreground | `oklch(0.205 0 0)` | #171717 |
| secondary / muted / accent / sidebar-accent | `oklch(0.269 0 0)` | **#262626** |
| muted-foreground | `oklch(0.708 0 0)` | **#a1a1a1** |
| destructive | `oklch(0.704 0.191 22.216)` | **#ff6467** |
| border / sidebar-border | `oklch(1 0 0 / 10%)` | **rgba(255,255,255,0.10)** |
| input | `oklch(1 0 0 / 15%)` | **rgba(255,255,255,0.15)** |
| ring | `oklch(0.556 0 0)` | **#737373** |

### Semantic accents (fixed sRGB, status/priority/due-date)
| name | hex | used for |
|---|---|---|
| neutral | #A1A1AA | no-priority, backlog/cancelled/duplicate, muted chrome |
| yellow | #FACC15 | medium priority; (icon tint for in-progress uses #EAB308, see below) |
| green | #22C55E | Done status |
| red | #EF4444 | Urgent priority |
| orange | #F97316 | High priority |
| blue | #3B82F6 | Low priority |

### Tailwind accents actually rendered in the web app
| use | class | hex |
|---|---|---|
| "New Issue" button | `bg-indigo-600` | **#4f46e5** (hover `indigo-700` #4338ca, white text) |
| In Progress icon/text | `text-yellow-500` | #eab308 |
| Done icon/text | `text-green-500` | #22c55e |
| Urgent | `text-red-500` | #ef4444 |
| High | `text-orange-500` | #f97316 |
| Medium | `text-yellow-500` | #eab308 |
| Low | `text-blue-500` | #3b82f6 |
| Video/marketing accent | indigo-500/400 | #6366f1 / #818cf8 |
| Project dot (screenshots) | indigo-ish violet | #6366f1 reads correct |

### Status-group header tints (issue-list.tsx `statusHeaderBg`)
```
backlog / cancelled / duplicate : rgba(113,113,122,0.08)   (zinc-500 @ 8%)
todo                            : rgba(212,212,216,0.08)   (zinc-300 @ 8%)
in_progress                     : rgba(234,179,8,0.10)     (yellow-500 @ 10%)
done                            : rgba(34,197,94,0.10)     (green-500 @ 10%)
```

### Marketing-site tokens (apps/marketing/src/styles/tokens.css) — same family
`--bg #0a0a0a · --bg-elev #171717 · --bg-soft #262626 · --fg #fafafa · --fg-muted #a1a1a1 ·
--fg-dim #737373 · --border rgba(255,255,255,.1) · --input rgba(255,255,255,.15)`;
primary button = **#e5e5e5 bg / #171717 text** (hover #d4d4d4). Status/priority same hexes as above.

---

## 3. The Exponential logo — EXACT geometry

A white (on dark) circle with three exponential-growth curves **cut out** of it (mask). ViewBox
`0 0 100 100`, full-bleed circle `cx=50 cy=50 r=50`. The three cut strokes (black in the mask,
clipped to the circle), `fill="none"`:

```svg
<svg viewBox="0 0 100 100">
  <defs>
    <clipPath id="c"><circle cx="50" cy="50" r="50"/></clipPath>
    <mask id="m">
      <rect width="100" height="100" fill="white"/>
      <g clip-path="url(#c)">
        <path d="M -5.87 62.01 C 39.09 65.44 48.72 28.71 49.03 -6.21" stroke="black" stroke-width="6" fill="none"/>
        <path d="M -5.07 86.00 C 53.78 84.42 71.13 37.29 73.00 -5.09" stroke="black" stroke-width="6" fill="none"/>
        <path d="M -4.27 109.99 C 68.46 103.40 93.55 45.86 96.98 -3.98" stroke="black" stroke-width="6" fill="none"/>
      </g>
    </mask>
  </defs>
  <circle cx="50" cy="50" r="50" fill="#ffffff" mask="url(#m)"/>
</svg>
```

Variants found in the repo:
- **On dark surfaces ("light" variant): disc fill `#ffffff`** — this is what the app sidebar / login use. (In-app component uses `currentColor` for the light variant.)
- On light surfaces: fill `#222326` (apps/web/public/logo-dark.svg) or `#0a0a0a` (marketing logo-dark.svg).
- Stroke width of the cuts: **6** in the standalone SVGs + marketing `ExpLogo`; the in-app React
  component (`apps/web/src/components/exponential-logo.tsx`) thins it to **3.5** for small sizes.
  For video at large sizes use **6**.
- Curves read as three swooshes rising left→right (exponential curves), denser at bottom-left.

⚠️ The current `Logo` in `apps/video/src/components.tsx` is a placeholder striped sphere and must be
replaced with the SVG above for a launch video.

### Wordmark
Plain text "Exponential" set in Inter, weight 600–700, tight tracking (−0.02em on marketing,
−1.5…−3 px at video display sizes). Lockup: mark left, wordmark right; gap ≈ 0.25× mark size
(app sidebar: 28px mark, gap 8px, 15–16px/600 text; video intro: 118px mark, gap 28, 108px/700 text).

### Copy strings (exact)
- Product name: `Exponential`
- Tagline (video): `The real-time issue tracker`
- Outro stack line: `Electric SQL · TanStack Start · React 19`
- App URL: `app.exponential.at` · Marketing: `exponential.at`
- Marketing CTA copy: `Go Exponential.` / `Free for individuals and teams. No credit card required.` /
  `Get started free` / `Sign up free` / `Self-host`

---

## 4. Typography

| context | family | notes |
|---|---|---|
| Product UI (web app) | **Inter** (Google Fonts, `wght@300..700`) | fallback `ui-sans-serif, system-ui, sans-serif` |
| Marketing site chrome | **Geist** (300–700) + **Geist Mono** | headers/pricing use mono for kickers |
| Marketing product recreations | Inter (UI) + **JetBrains Mono** (code) | `--font-ui` / `--font-code` |
| In-app monospace (issue identifiers, code) | `ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace` | identifiers like `MOB-1` render mono |
| Desktop/iOS/Android | Inter (via tokens.json `type.fontFamily`) | baseSize 16 |

### Loading fonts in Remotion (recommended)
```ts
import { loadFont } from "@remotion/google-fonts/Inter"
export const { fontFamily } = loadFont("normal", {
  weights: ["400", "500", "600", "700"],   // add 400 for body text in UI recreations
  subsets: ["latin"],
  ignoreTooManyRequestsWarning: true,
})
// For code/identifiers:
import { loadFont as loadMono } from "@remotion/google-fonts/JetBrainsMono"
// Optional marketing-style headline font:
import { loadFont as loadGeist } from "@remotion/google-fonts/Geist"
```
Current package loads only Inter 500/600/700 — sufficient for captions, NOT for UI recreation
(needs 400 body + a mono face for identifiers).

### ⚠️ Web root font-size gotcha (critical for pixel parity)
`apps/web/src/styles.css` sets `html { font-size: 1.15625rem }` → **1rem = 18.5 px** on desktop
(16px only below 768px). All Tailwind rem classes scale by ×18.5. The screenshots (1440 CSS px wide)
reflect this. Converted values below already use the 18.5 multiplier.

Type scale in the app at desktop root (rounded):
- `text-xs` = 0.75rem → **13.9px**, `text-sm` = 0.875rem → **16.2px**, `text-base` → **18.5px**,
  `text-2xl` = 1.5rem → **27.75px** (login "Sign in" title).
- Body line-height defaults; UI text weight 400–500, section titles 500–600, page titles 500.

---

## 5. Radii, spacing rhythm, control geometry

- `--radius: 0.625rem` → **11.5px rendered** on desktop web (tokens.json nominal: sm 6 / md 8 / lg 10 / xl 14 at 16px root).
- Buttons (shadcn, rendered px at 18.5 root): default h-9 → 41.6px; `sm` h-8 → 37px; `xs` h-6 → **27.75px**
  (px-2, text-xs, radius-md, icons size-3=13.9px); `icon-xs` 27.75px square.
- Inputs h-9 → 41.6px; list rows `h-10` → **46.25px** (mobile h-12 → 55.5px).
- Sidebar width: `16rem` → **296px** (mobile 18rem; icon-collapsed 3rem).
- Issue detail right rail: `w-72` → **333px**, `border-l rgba(255,255,255,0.10)`, padding 18.5px, `space-y-4`.
- Page gutters: `px-6` → 27.75px (desktop), `px-4` → 18.5px (mobile).
- Spacing rhythm = Tailwind ×18.5: gap-1 4.6 / gap-1.5 6.9 / gap-2 9.25 / gap-3 13.9 / gap-4 18.5 / gap-6 27.75.
- Icons: lucide-react, mostly `h-4 w-4` (18.5px) in nav, `size-3`/`h-3.5` (13.9/16.2px) in dense rows; stroke ~2.

---

## 6. Surface breakdowns (build targets)

All surfaces sit on `#0a0a0a`. Global text `#fafafa`, muted `#a1a1a1`/`#a1a1aa`. Screenshots are
1440×900 CSS px design size.

### 6.1 App shell
```
┌──────────────┬──────────────────────────────────────────────────────────────┐
│ SIDEBAR      │  MAIN                                                        │
│ 296px        │  fills rest, bg #0a0a0a                                      │
│ bg #171717   │                                                              │
│ border-r     │                                                              │
│ rgba(w,10%)  │                                                              │
└──────────────┴──────────────────────────────────────────────────────────────┘
```

### 6.2 Sidebar (from 02-board.png + sidebar.tsx)
```
┌────────────────────────────┐
│ ◍ Exponential              │  header h≈83px: logo 28px (white disc, cut curves)
│────────────────────────────│  + "Exponential" 16px/600; hairline separator below
│  🔍 Search                 │  nav rows: h≈37px, icon 18.5px + label 16.2px/400,
│  ◉ My Issues               │  gap 9px, px≈14px, radius 8, hover bg #262626
│  📥 Inbox                  │  icons: Search, CircleUser, Inbox, GitPullRequest,
│  ⑂ Reviews                 │  Bot, Rocket (lucide, 2px stroke, #fafafa)
│  🤖 Agents                 │  count badges right-aligned, muted
│  🚀 Releases               │
│                            │
│  Projects              +   │  group label 13.9px muted + Plus icon-btn right
│  ▦ Mobile App              │  project row: FolderKanban-ish icon tinted #6366f1
│            …               │
│  📣 Send feedback          │  pinned near bottom, muted
│  (AL) ada+run2@example.com │  footer: 28px round avatar (#262626 bg, 12px initials)
│                          ⇕ │  + email 16px + ChevronsUpDown
└────────────────────────────┘
```
Exact strings: `Search`, `My Issues`, `Inbox`, `Reviews`, `Agents`, `Releases`, `Projects`,
`Mobile App`, `Send feedback`. Avatar initials style: circle, bg #262626, text #fafafa 12px/500.

### 6.3 Issue board (02-board.png) — the hero surface
```
┌ Issues                                                    ☰ Filter  [+ New Issue] ┐  ← h≈65px, px 27.75
│ (All Issues) ( Active ) ( Backlog )                                               │  ← pill tabs h-7=32px, rounded-full,
│                                                                                   │    active bg #262626, inactive muted
├───────────────────────────────────────────────────────────────────────────────────┤
│ ▾ ⏱ In Progress  2                                                        [+]     │  ← group header, bg rgba(234,179,8,.10)
│  ⚠ MOB-2   ⏱  Push notifications flake on Android 14              (AL)  📅       │  ← row h 46.25px
│  ⟂ MOB-1   ⏱  Add offline mode for the issue list                 (AL)  📅       │
│ ▾ ○ Todo  2                                            bg rgba(212,212,216,.08)   │
│  ⟂ MOB-3   ○  Polish the onboarding wizard copy                    (·)  📅       │
│  ⟂ MOB-4   ○  Dark-mode contrast on due-date pills                (AL)  📅       │
│ ▾ ◌ Backlog  2                                         bg rgba(113,113,122,.08)   │
│  ⟂ MOB-6   ◌  Investigate cold-start latency                       (·)  📅       │
│  ⟂ MOB-5   ◌  Swipe-to-complete gesture on rows                    (·)  📅       │
│ ▾ ✓ Done  2                                            bg rgba(34,197,94,.10)     │
│  ⟂ MOB-7   ✓  Ship real-time typing indicators                    (AL)  📅       │
│  ⟂ MOB-8   ✓  Migrate to TanStack DB collections                  (AL)  📅       │
└───────────────────────────────────────────────────────────────────────────────────┘
```
- **Header bar**: title `Issues` 18.5px/500 left; right: ghost `Filter` button (funnel icon 13.9px +
  text-xs muted) then **New Issue** button — `size xs` h 27.75px, px 9.25, radius 8, **bg #4f46e5**,
  white text 13.9px/500, leading Plus icon 13.9px.
- **Tabs row**: labels exactly `All Issues` / `Active` / `Backlog`; h 32.4px, rounded-full, px 13.9,
  text 13.9px; active `bg #262626 #fafafa 500`, inactive `#a1a1a1`.
- **Group header**: sticky, `pl-3 pr-6 py-1.5` (13.9/27.75/6.9px), bottom border rgba(255,255,255,0.05);
  contents: chevron ghost btn 23px (ChevronRight, rotates 90° open, 13.9px icon, muted) · status icon
  16.2px tinted (see mapping) · label 16.2px/500 `#fafafa` · count 13.9px muted; hover-only `+` icon-btn right.
- **Row** (desktop grid `1.5rem 4.5rem 1.5rem 1fr auto 1.75rem 4.5rem` → 27.75 | 83.25 | 27.75 | 1fr | auto | 32.4 | 83.25 px,
  `px-6`, h 46.25px, bottom border **rgba(255,255,255,0.03)**, hover bg rgba(38,38,38,0.30)):
  1. priority icon (centered, 15–16px)
  2. identifier — mono 13.9px `#a1a1a1` (`MOB-1` …)
  3. status icon (centered)
  4. title — 16.2px `#fafafa`, truncates, `ml-2`
  5. label pills — border rgba(255,255,255,0.05), rounded-full, px 6.9/py 1, text 13.9px muted, 6.9px color dot
  6. assignee avatar — 26px circle, bg #262626, initials `AL` ~10px; unassigned = dashed-ish muted user icon
  7. due date — CalendarDays icon 16px muted (right aligned)
- **Status icon mapping** (lucide): backlog `CircleDashed` #a1a1a1 · todo `Circle` #fafafa ·
  in_progress `Timer` #eab308 · done `CircleCheck` #22c55e · cancelled `CircleX` muted · duplicate `Copy` muted.
- **Priority mapping**: none `Minus` muted · urgent `AlertTriangle` #ef4444 · high `SignalHigh` #f97316 ·
  medium `SignalMedium` #eab308 · low `SignalLow` #3b82f6. (Signal icons = ascending bars.)

### 6.4 Issue detail (03-issue.png)
```
┌ ● Mobile App › MOB-1 › Add offline mode for the issue list   2 / 8 ∧ ∨ | 🔗  🔕 Subscribe ┐ ← top bar h≈65, border-b
├──────────────────────────────────────────────────────────┬────────────────────────────────┤
│ (Details)(Changes)  ← pill tabs, active #262626          │ STATUS      ⏱ In Progress      │
│                                                          │ PRIORITY    ⟂ High (#f97316)   │
│ **Add offline mode for the issue list**  ← title ~21px/600│ LABELS      ◔ Label            │
│ H1 H2 H3 | B I S ‹› | 🔗 ❝ | • 1. ☑ | Tx | 🖼  ← toolbar  │ RELEASE     🚀 Release         │
│ Cache the last synced issues locally so the list renders │ DUE DATE    📅 Due date        │
│ instantly on cold start.   ← body 16.2px                 │             ⟳ Add recurrence   │
│ ──────────────────────────────────────────               │ PROJECT     (● Mobile App)     │
│ 📎                                    0 images           │                                │
│ ──────────────────────────────────────────               │  right rail w-72 = 333px,      │
│ Activity                                                 │  border-l rgba(w,0.10),        │
│ No activity yet. Be the first to add a comment.          │  labels 13.9px uppercase muted │
│ ┌──────────────────────────────────────────────┐  [➤]   │  values 16.2px w/ 16px icons   │
│ │ Leave a reply...                              │        │                                │
│ └──────────────────────────────────────────────┘        │                                │
└──────────────────────────────────────────────────────────┴────────────────────────────────┘
```
- Breadcrumb: project dot (8px, #6366f1) + `Mobile App` 16.2px + `›` muted + `MOB-1` mono muted + `›` + title.
- Right of top bar: pager `2 / 8` muted + chevron-up/down ghost buttons + divider + link icon + `Subscribe`
  with BellOff icon (ghost, muted).
- Tabs `Details` / `Changes` — same pill style as board tabs (active bg #262626).
- Editor toolbar icons (order): H1 H2 H3 | Bold Italic Strikethrough Code | Link Quote | BulletList
  OrderedList TaskList | ClearFormatting | Image — ghost icon buttons ~27.75px, muted, thin separators.
- Reply box: bordered rgba(255,255,255,0.15), radius ~11.5, bg #171717-ish (input/30), placeholder muted
  `Leave a reply...`; send icon button (paper-plane) 46px square secondary at right.
- Rail sections exactly: `STATUS`, `PRIORITY`, `LABELS`, `RELEASE`, `DUE DATE`, `PROJECT`; project value is a
  secondary pill (bg #262626, radius 8) with dot + name.

### 6.5 My Issues (04-my-issues.png)
Identical to board but title `My Issues`, no New Issue button (Filter only), only groups with content
(In Progress 2 / Todo 1 / Done 2 — empty groups hidden).

### 6.6 Inbox (05-inbox.png)
Top bar: Bell icon + `Inbox` 18.5px/600, centered content empty state:
56px circle outline (#262626 bg, check icon) → `All caught up` ~21px/600 →
`Assignments, comments and mentions on issues you follow will show up here.` 16.2px muted, centered, max-w ~480px.

### 6.7 Login (01-login.png)
Centered column on #0a0a0a:
- Brand lockup: logo 28–32px (white variant) + `Exponential` ~19px/600, centered, ~48px above card.
- Card: `max-w-sm` → **444px** wide (24rem×18.5), bg #171717, radius ~13px, border rgba(255,255,255,0.10),
  padding ~28px.
- `Sign in` 27.75px/600 centered → `Enter your email and password to continue` 16.2px muted →
  labels `Email` / `Password` 16.2px/500 → inputs h 41.6px (bg input/30 ≈ rgba(255,255,255,0.045),
  border rgba(255,255,255,0.15), radius 8, placeholder `you@example.com` / `Password`, eye toggle right) →
  primary button full-width h 41.6px **bg #e5e5e5, text #171717**, `Sign in` →
  `Don't have an account? Register` (muted + white link).
- Below card: `Privacy · Terms` 13.9px muted.

### 6.8 Marketing-style chrome (for mixed launch-video shots)
Topbar: brand (logo 22 + `Exponential` 15px/600 ls −0.02em) · nav `Product Pricing Docs Download` ·
ghost `GitHub` `Sign in` + primary `Get started free` (#e5e5e5/#171717). Marketing uses Geist for chrome;
product recreations inside it use Inter 13px + JetBrains Mono.

---

## 7. Motion language (existing, keep for consistency)

- Single easing: `cubic-bezier(0.16, 1, 0.3, 1)`.
- Scene envelope: 14-frame fade-in, 12-frame fade-out; sequences overlap 8 frames.
- Entrances: fade + 16–46px rise, 0.97→1 scale, 16–30 frames; staggers of 4–20 frames
  (kicker → headline → window; logo → wordmark → tagline).
- Ambient: background glow drifts +4% x over 300 frames.
- Durations at 30fps: brand cards ~2.7s, product shots 4–6.3s each.

## 8. Gotchas / recommendations for the rebuild

1. **Replace the placeholder Logo component** with the real cut-curve SVG (§3, stroke-width 6, white disc).
2. Give React SVG mask/clip ids unique names per instance (the web component uses `useId`).
3. Canvas bg: video currently `#09090b`; the app itself is `#0a0a0a` — visually identical, but recreated
   UI panels must use `#0a0a0a` / `#171717` / `#262626`, borders at white 10% (5% inner separators / 3% row hairlines).
4. All rem-based sizes in the web app scale ×18.5/16; the pixel values in §6 are already converted —
   don't re-derive from stock Tailwind px.
5. Identifiers (`MOB-1`, `EXP-42`) are monospace; load JetBrains Mono (or use a system-mono stack) in Remotion.
6. Inter should also load weight 400 for body copy in UI recreations.
7. Keep Remotion pinned at 4.0.484; register new compositions in `src/Root.tsx`.
8. Screenshots in `public/shots/` are 2× (2880×1800): display at ≤1440 logical px or crop; the two unused
   shots (my-issues, inbox) are available for extra scenes.
9. The app never shows an indigo accent except: New Issue button (#4f46e5), project icon/dot (#6366f1),
   and sidebar-primary token; the brand's video accent glow (#6366f1/#818cf8) is a video-only treatment.
