# Exponential Web App — Pixel-Perfect UI Recreation Spec

Source of truth for recreating the Exponential issue-tracker web UI in React/Remotion.
Derived from `apps/web/src` (styles.css, shadcn ui components, workspace sidebar, issue list/detail, dialogs, agent session, inbox). Everything below is measured from the actual source code.

---

## 1. Global Design Tokens

### 1.1 Theme mode
The app is **dark-theme forced**: `<html lang="en" class="dark">`. All values below are the `.dark` token set unless noted. There is no theme toggle in the app UI.

### 1.2 Root font scaling (IMPORTANT for pixel parity)
```css
html { font-size: 1.15625rem; }        /* = 18.5px root on ≥768px viewports */
@media (max-width: 767px) { html { font-size: 1rem; } }  /* 16px on mobile */
```
**Every Tailwind rem value is multiplied by 18.5/16 = 1.15625 on desktop.**
Examples at desktop scale: `text-sm` (0.875rem) ≈ **16.2px**, `text-xs` (0.75rem) ≈ **13.9px**, `h-10` (2.5rem) ≈ **46.3px**, `h-8` (2rem) = 37px, sidebar `16rem` = **296px**.
For a Remotion recreation you can either replicate the 18.5px root or bake the multiplied px values in; the ratios below are given in rem + the effective desktop px.

### 1.3 Font
- Family: `"Inter", ui-sans-serif, system-ui, sans-serif` (Google Fonts, weights 300–700, `display=swap`).
- Monospace (identifiers, diffs, code, `#EXP-42` pills): `ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace`.
- Body: antialiased, weight 400 default. Headings/medium emphasis use 500/600.

### 1.4 Core color tokens (dark theme) — OKLCH → hex
Neutral scale is chroma-0 OKLCH (equals Tailwind **neutral**, not zinc, despite docs saying zinc):

| Token | OKLCH | Hex |
|---|---|---|
| `--background` | `oklch(0.145 0 0)` | **#0a0a0a** |
| `--foreground` | `oklch(0.985 0 0)` | **#fafafa** |
| `--card` / `--popover` / `--sidebar` | `oklch(0.205 0 0)` | **#171717** |
| `--card-foreground` / `--popover-foreground` / `--sidebar-foreground` | `oklch(0.985 0 0)` | #fafafa |
| `--primary` | `oklch(0.922 0 0)` | **#e5e5e5** (light gray — the "white" button) |
| `--primary-foreground` | `oklch(0.205 0 0)` | #171717 |
| `--secondary` / `--muted` / `--accent` / `--sidebar-accent` | `oklch(0.269 0 0)` | **#262626** |
| `--secondary-foreground` / `--accent-foreground` | `oklch(0.985 0 0)` | #fafafa |
| `--muted-foreground` | `oklch(0.708 0 0)` | **#a3a3a3** |
| `--destructive` | `oklch(0.704 0.191 22.216)` | **#f87171** (red-400) |
| `--border` / `--sidebar-border` | `oklch(1 0 0 / 10%)` | **rgba(255,255,255,0.10)** (reads ~#232323 on bg) |
| `--input` | `oklch(1 0 0 / 15%)` | rgba(255,255,255,0.15) |
| `--ring` | `oklch(0.556 0 0)` | #737373 |
| `--link` (editor links) | `oklch(0.65 0.15 264)` | ≈ **#7c86e8** (muted indigo) |

Light theme exists in tokens (`#ffffff` bg, `#0a0a0a` fg, `#f5f5f5` muted, `#e5e5e5` border) but is never shown — recreate dark only.

### 1.5 Accent / semantic colors (Tailwind palette classes used directly)
| Use | Class | Hex |
|---|---|---|
| Brand / CTA ("New Issue", create buttons) | `bg-indigo-600` hover `bg-indigo-700`, white text | **#4f46e5** / hover #4338ca |
| Filter count chip | `bg-indigo-500/20 text-indigo-400` | bg rgba(99,102,241,0.2), text #818cf8 |
| In-progress status | `text-yellow-500` | #eab308 |
| Done status | `text-green-500` | #22c55e |
| Urgent priority | `text-red-500` | #ef4444 |
| High priority | `text-orange-500` | #f97316 |
| Medium priority | `text-yellow-500` | #eab308 |
| Low priority | `text-blue-500` | #3b82f6 |
| Live/coding indicators, diff additions | `emerald-500` #10b981, `emerald-400` #34d399, `emerald-300` #6ee7b7 |
| Diff deletions | `rose-400` #fb7185, `rose-300` #fda4af |
| Diff modified badge | `amber-400` #fbbf24 |
| Diff renamed badge | `sky-400` #38bdf8 |
| Diff hunk header | `text-indigo-300/80` on `bg-indigo-500/5` |
| Connecting pulse dot | `amber-400` #fbbf24 |

### 1.6 Radii
`--radius: 0.625rem` (≈11.6px at 18.5px root). Derived: `radius-sm` −4px, `md` −2px, `lg` = radius, `xl` +4px.
In practice: buttons/cards/menus `rounded-md`, dialogs `rounded-lg`, pills/badges/avatars `rounded-full`, inline code 3px, code blocks 6px, editor images 12px.

### 1.7 Spacing rhythm
Tailwind default 0.25rem grid (×1.15625 on desktop). Common paddings: page gutters `px-4 md:px-6`, list row `px-3 md:px-6`, panel sections `px-4 py-3`, compact bars `px-3 py-1.5` or `px-4 py-2`. Sidebar groups `p-2`, menu item gap `gap-1`.

### 1.8 Icons
All icons are **lucide-react**, stroke style (2px stroke), sized via Tailwind: `size-4` (1rem ≈18.5px) standard, `size-3`/`h-3.5 w-3.5` for compact/status glyphs, `size-2.5` micro.

---

## 2. App Shell Layout

```
┌──────────────────────────────────────────────────────────────────────────────┐
│┌────────────┐┌──────────────────────────────────────────────────────────────┐│
││  SIDEBAR   ││  MAIN (flex-1, flex-col, min-h-screen)                       ││
││  16rem     ││  ┌─────────────────────────────────────────────────────────┐ ││
││  (296px)   ││  │ page content (Outlet): board / detail / inbox / …       │ ││
││  bg        ││  │ bg #0a0a0a                                              │ ││
││  #171717   ││  └─────────────────────────────────────────────────────────┘ ││
││ border-r   ││                                                              ││
│└────────────┘└──────────────────────────────────────────────────────────────┘│
└──────────────────────────────────────────────────────────────────────────────┘
```
- Sidebar: fixed, full height (`h-svh`), width `16rem` = **296px** desktop, background `--sidebar` **#171717**, right border `rgba(255,255,255,0.10)`.
- Main content background **#0a0a0a**.
- Mobile (<768px): sidebar becomes an 18rem sheet; a mobile topbar appears (not needed for a desktop launch video).
- Keyboard: Cmd/Ctrl+F opens global search dialog; Cmd/Ctrl+B toggles sidebar; J/K next/prev issue on detail.

---

## 3. Sidebar (`components/workspace/sidebar.tsx`)

Structure top→bottom:

```
┌──────────── SIDEBAR 296px, #171717 ─────────────┐
│ p-2 header                                       │
│ ┌──────────────────────────────────────────────┐ │
│ │ [W] Workspace Name              ⇅ chevrons   │ │  h-10 workspace switcher button
│ └──────────────────────────────────────────────┘ │
│ ─────────────── separator ──────────────────────  │
│  (SidebarContent, scrollable)                    │
│  group p-2:                                      │
│   🔍  Search                                     │  each row: h-8, rounded-md,
│   👤  My Issues                                  │  px-2 (p-2), gap-2, text-sm,
│   📥  Inbox                              [3]     │  icon size-4, hover bg #262626
│   ⇄   Reviews                            [2]     │  (GitPullRequest icon)
│   🤖  Agents                             [1]     │  (Bot icon)
│   🚀  Releases                           [4]     │  (Rocket icon)
│                                                  │
│  group p-2:                                      │
│   PROJECTS                               [+]     │  group label h-8 px-2 text-xs
│   </> Exponential                    🌐(if fb)   │  font-medium #fafafa/70
│   ▦   Personal Tasks                             │  project icon tinted project.color
│   📣  Feedback Board                             │
│                                                  │
│  (SidebarFooter, p-2)                            │
│   ⬇  Download desktop app   (members only)      │
│   💬 Feedback                                    │
│ ┌──────────────────────────────────────────────┐ │
│ │ (DS) dennis@straehhuber.com          ⇅       │ │  user row: avatar h-6 w-6
│ └──────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────┘
```

Details:
- **Workspace switcher** (header, `p-2`): a `SidebarMenuButton` `h-10 w-full` containing a **7×7 (`h-7 w-7`) rounded-md tile** `bg-primary` (#e5e5e5) with the workspace initial in `text-primary-foreground text-xs font-bold` (i.e. dark letter on light tile), then workspace name `text-sm font-semibold truncate`, then `ChevronsUpDown h-4 w-4` right-aligned. Solo users instead see the **Exponential logo** (circle with three swooping curve cutouts, `currentColor`, 28px) + "Exponential" `text-sm font-semibold`.
- Below header: 1px `Separator` (full-width, border color).
- **Nav rows** (`SidebarMenuButton` default): `h-8` (37px), `p-2`, `gap-2`, `rounded-md`, `text-sm`, icons `size-4`. Hover/active: `bg-sidebar-accent` #262626. Exact labels + lucide icons in order: `Search` (Search), `My Issues` (CircleUser), `Inbox` (Inbox), `Reviews` (GitPullRequest), `Agents` (Bot), `Releases` (Rocket).
- **Badges** (`SidebarMenuBadge`): absolutely positioned `right-1`, `h-5 min-w-5 px-1`, `rounded-md`, `text-xs font-medium tabular-nums`, plain foreground color (no background). Shown for: Inbox = unread notification count, Reviews = open-PR count, Agents = running coding sessions, Releases = unshipped release count. Caps at `99+`. Hidden at zero.
- **Projects group**: `SidebarGroupLabel` "Projects" — `h-8 px-2 text-xs font-medium`, color `#fafafa` at 70% opacity. A `+` `SidebarGroupAction` (Plus size-4) absolutely at `top-3.5 right-3`, 5-unit square, hover bg #262626. Each project row = menu button with the project-type icon **tinted `project.color`** (arbitrary per-project hex): `dev` → `Code2`, `tasks` → `SquareKanban`, `feedback` → `Megaphone`; feedback projects additionally get a right-aligned `Globe h-3.5 w-3.5 text-muted-foreground`. Empty state: disabled row `FolderKanban` + "No projects yet" in muted.
- **Footer**: desktop-download entry + "Feedback" button (styled as menu rows), then the **user button**: `Avatar h-6 w-6` (rounded-full, `AvatarFallback` bg muted #262626, initials `text-xs`), email `text-sm truncate`, `ChevronsUpDown h-4 w-4` right. Opens a dropdown (side=top) with: `Admin` (Shield), `Settings` (Settings, solo mode), `Account & notifications` (Bell), separator, `Sign out` (LogOut) — items `mr-2 h-4 w-4` icons.
- Anonymous visitors see a full-width primary button `Sign in to contribute` (LogIn icon) instead.

Dropdown menus (all app-wide): `bg-popover` #171717, `rounded-md`, 1px border rgba(255,255,255,.1), shadow-md, item height ~2rem, `text-sm`, `w-56` for the workspace/user menus.

---

## 4. Project Board Page (filter bar + grouped issue list)

Route: `/w/:workspaceSlug/projects/:projectSlug`. Column layout: `IssueFilterBar` on top, scrollable `IssueList` below.

### 4.1 Filter bar (`issue-filter-bar.tsx`)
Container `px-4 md:px-6`.

```
Issues                                    [⧩ Filter]  [+ New Issue]     ← row 1, py-3
(All Issues) (Active) (Backlog)                                         ← row 2, tab pills
[◔ In Progress ×] [⚠ Urgent ×] [● bug ×]  Clear all                    ← row 3, only when filtered
```
- Row 1: title `h1` `text-base font-medium` ("Issues"); right-aligned controls `gap-1`:
  - **Filter button**: ghost `size-xs` (h-6 px-2 text-xs, muted-foreground) with `ListFilter size-3` + label `Filter`; when filters active, a count chip `rounded-full bg-indigo-500/20 text-indigo-400 px-1.5 text-[0.625rem] font-medium`.
  - **New Issue button**: `size-xs` (h-6, rounded-md, px-2, text-xs) `bg-indigo-600 hover:bg-indigo-700 text-white`, `Plus size-3` + `New Issue`.
- Row 2 tabs: ghost buttons, `rounded-full h-7 px-3 text-xs`; active tab = `bg-accent (#262626) text-foreground font-medium`; inactive = `text-muted-foreground`. Labels exactly: `All Issues`, `Active`, `Backlog`.
- Row 3 active-filter pills (`active-filter-pills.tsx`): `px-6 py-1.5 gap-1.5 flex-wrap`. Each pill = outline button `h-6 rounded-full text-xs gap-1` (1px border, `bg-input/30`): status pills show the status icon at 3×3 + label; priority pills same; label pills show a `h-2 w-2 rounded-full` dot in `label.color` + name; all end with `X size-2.5`. Trailing ghost `Clear all` (h-6 text-xs muted).
- Filter popover: `w-[14rem] p-0` popover with drill-down list (Status ▸ / Priority ▸ / Labels ▸ with checkboxes).

### 4.2 Issue list (`issue-list.tsx`) — grouped by status

Groups render in status order (backlog, todo, in_progress, done, cancelled, duplicate), **empty groups hidden**.

**Group header** (sticky top-0, z-10):
```
[›] ◔ In Progress  7                                              [+]
```
- Layout: `flex justify-between`, `pl-3 pr-3 md:pr-6 py-1.5`, bottom border `border-border/50`.
- Tinted background per status (exact rgba):
  - backlog / cancelled / duplicate: `rgba(113,113,122,0.08)` (zinc-500 @8%)
  - todo: `rgba(212,212,216,0.08)` (zinc-300 @8%)
  - in_progress: `rgba(234,179,8,0.10)` (yellow-500 @10%)
  - done: `rgba(34,197,94,0.10)` (green-500 @10%)
- Contents `gap-1.5`: collapse chevron (ghost 5×5 md button, `ChevronRight size-3`, rotates 90° when open, 200ms), status icon `h-3.5 w-3.5` in status color, status label `text-sm font-medium`, count `text-xs text-muted-foreground`. Hover-only `+` button on the right (ghost icon-xs, `Plus size-3`).

**Status icon/color/label map** (lucide):
| status | label | icon | color |
|---|---|---|---|
| backlog | Backlog | `CircleDashed` | muted #a3a3a3 |
| todo | Todo | `Circle` | foreground #fafafa |
| in_progress | In Progress | `Timer` | yellow-500 #eab308 |
| done | Done | `CircleCheck` | green-500 #22c55e |
| cancelled | Cancelled | `CircleX` | muted #a3a3a3 |
| duplicate | Duplicate | `Copy` | muted #a3a3a3 |

**Priority icon/color/label map**:
| priority | label | icon | color |
|---|---|---|---|
| none | No priority | `Minus` | muted #a3a3a3 |
| urgent | Urgent | `AlertTriangle` | red-500 #ef4444 |
| high | High | `SignalHigh` | orange-500 #f97316 |
| medium | Medium | `SignalMedium` | yellow-500 #eab308 |
| low | Low | `SignalLow` | blue-500 #3b82f6 |

**Issue row** — CSS grid, height `h-10` desktop (**2.5rem ≈ 46px**; `h-12` mobile), `px-3 md:px-6`, bottom border `border-border/30`, hover `bg-accent/30` (#262626 @30%), cursor-pointer.

Desktop grid template (bulk-select enabled boards):
`grid-cols-[1.25rem_1.5rem_4.5rem_1.5rem_1fr_auto_1.75rem_4.5rem]`
```
[✓] [prio] [EXP-42  ] [stat] [Title text…        ] [labels] [👤] [   Jul 12 📅]
 1.25  1.5    4.5rem    1.5     1fr                  auto    1.75    4.5rem
```
(The historical simple layout the docs cite — `24px_72px_24px_1fr_auto` — evolved into this; keep the 8-column version.)
Cells left→right:
1. **Checkbox** (md+, only when bulk select on): shadcn checkbox, invisible until row hover (`opacity-0 group-hover:opacity-100`), stays visible while any selection exists.
2. **Priority button**: ghost 5×5 (`md:h-5 md:w-5 p-0`), icon `h-3.5 w-3.5` in priority color. Click opens priority dropdown.
3. **Identifier**: `text-xs text-muted-foreground font-mono truncate` — e.g. `EXP-42`.
4. **Status button**: ghost 5×5, icon `h-3.5 w-3.5` in status color, opens status dropdown.
5. **Title**: `text-sm truncate ml-2`; recurring issues prefix a `Repeat size-3` muted icon.
6. **Labels** (md+): `gap-1.5 ml-4`; each label chip = `border border-border/50 rounded-full px-1.5 py-px text-xs text-muted-foreground` with a leading `h-1.5 w-1.5 rounded-full` color dot.
7. **Assignee**: 5×5 `Avatar` (image or initials fallback `text-[0.625rem]` on #262626) or, unassigned, a **dashed-border circle** `size-5 border-dashed` with `UserIcon size-2.5` at 50% muted.
8. **Due date**: right-aligned; `CalendarDays size-3` (muted when set, muted/30 when not) + date `text-xs text-muted-foreground` formatted `Jul 12` (`en-US`, short month + day).

Right-click row → context menu (status/priority/assignee/labels/release submenu/move-to-project/delete-with-confirm-submenu). Selecting rows shows a floating **BulkActionBar** bottom-center.

**Empty states** (`empty-state.tsx`): centered column `py-12 gap-3`; 12×12 circle `bg-primary/10` with `size-6 text-primary` icon; `text-lg font-semibold` title; `text-sm text-muted-foreground` description. Variants: `ListTodo` / "No issues yet" / "Create an issue to start tracking work." + indigo New issue button; `SearchX` / "No issues match your filters" / "Try removing some filters to see more issues." + outline `Clear filters`.

**Loading skeleton**: one fake group header (round 3.5 skeleton + 24-wide bar) and 5 rows (`h-12 md:h-10`) with circle+bar skeletons, `bg-accent` pulse.

---

## 5. Issue Detail Page

Route: `/w/:ws/projects/:proj/issues/:identifier`. Full-page (not a modal).

```
┌────────────────────────────────────────────────────────────────────────────┐
│ ● Exponential › EXP-42 › Fix sync bug        3 / 27  ▲ ▼ | 🔗 🔔 ⋯        │ breadcrumb bar
├────────────────────────────────────────────────────────────────────────────┤
│ (optional) 📄 Duplicate of  (#EXP-12)  Original title…            Unmark   │ duplicate banner
├───────────────────────────────────────────────────────┬────────────────────┤
│ [Details] [Changes ●]                                 │ STATUS             │
│ ┌──── centered column max-w-3xl ─────────────┐        │  ◔ In Progress     │
│ │ Fix sync bug                (title, 2xl)   │        │ PRIORITY           │
│ │ B I S </> H1 H2 H3 … (toolbar)             │        │  ⚠ Urgent          │
│ │ Description markdown body…                 │        │ ASSIGNEE           │
│ │                                            │        │  (DS) Dennis       │
│ │ ── attachment rail ──                      │        │ LABELS             │
│ │ ── Activity (4) ──────────────             │        │  ● bug  + Add      │
│ │  ◉ Dennis changed status to done           │        │ RELEASE            │
│ │  (DS) Dennis  2h                           │        │  🚀 v1.2           │
│ │      Comment body markdown                 │        │ DUE DATE           │
│ │  [Leave a reply…            ] [➤]          │        │  📅 Jul 12         │
│ └────────────────────────────────────────────┘        │  ↻ Add recurrence  │
│                                                       │ PROJECT            │
│                                                       │  ● Exponential     │
└───────────────────────────────────────────────────────┴────────────────────┘
                                                          ← aside w-72, border-l
```

### 5.1 Breadcrumb bar
`flex items-center gap-1.5 text-xs text-muted-foreground px-4 py-2 border-b`:
- Project link: `h-2.5 w-2.5 rounded-full` dot in project color + project name (hover → foreground).
- `ChevronRight size-3` at 50% muted between segments.
- Identifier in `font-mono`; title `text-foreground truncate`.
- Right cluster `gap-1`: position `3 / 27` (`font-mono tabular-nums`), `ChevronUp`/`ChevronDown` ghost icon-xs prev/next (J/K), vertical separator, copy-link `Link2 size-4` (flips to green-ish `Check text-primary` for 1.5s), subscribe bell toggle, `MoreHorizontal` overflow (Unmark duplicate / Delete issue ▸ Confirm delete in destructive red).

### 5.2 Tabs bar
`px-4 py-2 border-b`. Segmented control: wrapper `inline-flex gap-0.5 rounded-lg bg-muted/50 p-0.5`; each tab ghost `h-6 rounded-md px-3 text-xs`; active = `bg-background text-foreground shadow-sm`, inactive = muted. Labels: `Details`, `Changes`. When a PR/branch/session exists, Changes gets a `size-1.5` emerald-500 dot (with emerald-400 ping animation while coding live).

### 5.3 Details column (centered `max-w-3xl`)
- **Title**: borderless Input, `text-2xl font-semibold px-5 pt-4 pb-1`, placeholder "Issue title" at 50% muted.
- **Editor toolbar** (`.static-toolbar`): 28×28 icon buttons, radius 5px, muted → hover bg #262626; 1×16px separators; bottom border; sticky in dialogs.
- **Description** (TipTap): `text-sm (0.875rem)`, line-height 1.625, `padding: 0.25rem 1.25rem`, min-height 3.5rem, placeholder "Add description...". Markdown styling: H1 1.25rem/600, H2 1.1rem/600, H3 1rem/600; code `bg-muted` 3px radius `0.8rem`; pre `bg-muted` 1px border 6px radius; blockquote 2px left border muted text; task-list checkboxes accent `--primary`; images 12px radius 1px border; `#EXP-n` refs & `@mentions` render as pills: `bg-accent`, 1px border, `rounded-full`, `padding .05rem .4rem`, refs in mono `0.75rem`, mentions `0.8125rem/500`.
- **Attachment rail**: `px-4 py-3 border-t` — thumbnails of embedded images + add button.
- **Activity / timeline** (`issue-timeline.tsx`): `border-t px-4 py-3`. Header `Activity (N)` `text-xs font-medium muted`. Items sorted chronologically, mixing:
  - **Event rows**: one line, `gap-2 py-1 text-xs text-muted-foreground`, `size-3.5` icon + `<Actor>` (`font-medium text-foreground`) + text. Icons: status `CircleDot`, assignee `UserPlus`, label `Tag`, PR opened `GitPullRequest`, PR merged `GitMerge`, release `Rocket`, moved `FolderInput`. Strings: "changed status to **done**", "assigned **Name**", "added label **bug**", "opened a pull request", "merged the pull request", "added this to release **v1.2**".
  - **Comment rows**: `flex gap-2.5 py-2`; `Avatar h-7 w-7` initials; header `text-xs`: name `font-medium` + relative time (`2h`, `3d`) + `· edited`; hover `MoreHorizontal` menu (Edit/Delete); body rendered as read-only markdown `text-sm`.
  - **Composer**: textarea `min-h-16 text-sm` placeholder `Leave a reply…` + square primary send button (`size-9`, `Send size-4`). Cmd+Enter submits.

### 5.4 Properties sidebar (desktop)
`<aside class="w-72 shrink-0 border-l px-4 py-4 space-y-4 text-sm">` (72 = 18rem ≈ 333px).
Groups: label `text-[11px] font-medium uppercase tracking-wide text-muted-foreground` + control below. Order: **Status, Priority, Assignee (hidden when solo), Labels, Release, Due date (+ recurrence), Project**.
Controls are ghost xs buttons (`h-6 px-2 text-xs gap-1`, muted → hover foreground), left-justified: 3×3 status/priority icon + label; assignee = avatar+name; due date = `CalendarDays size-3` + `Jul 12` (opens calendar popover with time inputs); recurrence = `Repeat size-3` + `Add recurrence`/`Every 2 weeks`; project chip = `rounded-md bg-accent/40 px-2 py-1 text-xs font-medium` with color dot + name.

### 5.5 Changes tab
Wider column `max-w-5xl`. Shows PR diff / branch diff (see §8) or the live steer viewer (§7).

---

## 6. Create Issue Dialog (`create-issue-dialog.tsx` + `issue-editor/dialog-shell.tsx`)

Desktop: centered Dialog, `sm:max-w-[40rem]` (≈740px), `p-0`, `rounded-lg`, 1px border, `bg-background` #0a0a0a, `max-h-[85vh]`, overlay `bg-black/50`.

```
┌──────────────────────────── 40rem ────────────────────────────┐
│ (● EXP) › New issue                                       [×] │  px-5 pt-4 pb-2
│ Issue title                                                   │  text-lg font-medium, borderless
│ B I S … toolbar                                               │
│ Add description...                                            │  scrollable editor region
│                                                               │
├───────────────────────────────────────────────────────────────┤
│ [◌ Backlog] [− No priority] [👤] [🏷 Labels] [📅 Due date] [⋯] │  chip row, px-4 py-2, border-t
├───────────────────────────────────────────────────────────────┤
│ (attachment rail)          [◯ Create more]  [Create issue]    │  footer px-4 py-3, border-t
└───────────────────────────────────────────────────────────────┘
```
- Header: project pill (`rounded-md bg-accent/50 px-2 py-0.5 text-xs font-medium` with `h-2.5 w-2.5` project-color dot + prefix e.g. `EXP`), `ChevronRight h-3 w-3`, "New issue" `text-sm`, close `X size-3` ghost icon-xs top-right.
- Title input: borderless, `text-lg font-medium px-5 py-1`, autofocus; Tab jumps into the editor.
- Chip row: ghost xs chips (muted): status (icon 3×3 + label, "duplicate" excluded), priority, assignee (hidden solo), labels, due date (`CalendarDays size-3` + "Due date"/formatted), overflow `⋯` menu with `Make recurring…` (Repeat icon).
- Footer right cluster `gap-3`: small **Switch** + label `Create more` (`text-xs muted`), then submit button: `h-7 rounded-md bg-indigo-600 hover:bg-indigo-700 px-3 text-xs font-medium text-white` — label `Create issue` → `Creating...` → `Uploading images...`. Recurring variant: footer shows recurrence editor left, button `Create recurring issue`.

---

## 7. Live Agent Session / Steer View (`agent-session.tsx`)

Rendered inside the issue Changes tab and on the workspace **Agents** page. This is the flashiest surface: a live feed of a Claude coding session streamed from the desktop app.

**Header row above the panel** (when a session runs), `border-t px-4 py-3`:
- `Badge variant=outline` `rounded-full px-2 py-0.5 text-xs gap-1.5` with `border-emerald-500/40 text-emerald-400`: a `size-2` emerald-500 dot with emerald-400 **ping animation** + text `Coding now` (+ ` (2)` when multiple). Next to it `text-xs muted`: `Dennis · MacBook Pro` (owner · deviceLabel).

**Controls row** (`gap-2`): outline sm button `Watch live` (MonitorPlay icon) / `Reconnect` (RotateCw); **phase indicator**: `size-2` dot — emerald-500 = `Live · MacBook Pro`, pulsing amber-400 = `Agent starting…`/`Connecting…`, muted = `Session ended`/`Disconnected` — with label `text-xs muted`; ghost sm destructive `Kill session` (OctagonX); right-aligned ghost `Close` (X).

**The panel**: `h-96` (24rem ≈ 444px) column, `rounded-md border bg-card/40`, sections top→bottom:
1. **Activity feed** (flex-1, scroll, bottom-anchored `justify-end gap-0.5 px-3 py-2`):
   - **NarrationBubble** (assistant prose): `Sparkles size-3` glyph at 60% muted + bubble `rounded-md border-border/60 bg-muted/30 px-3 py-1.5 text-sm text-foreground/90 whitespace-pre-wrap`.
   - **ToolRow** (tool headline): one tight line `py-0.5` — `Wrench size-3` 60% muted + tool name `text-xs font-medium` + detail `font-mono text-[0.6875rem] muted truncate` (e.g. `Bash` `bun run test`).
   - Empty/connecting states centered with spinner: "Connecting…", "The agent is starting — waiting for the live stream…", "Waiting for activity…".
   - When scrolled up: floating pill `Jump to latest ↓` (secondary sm rounded-full, bottom-center, shadow).
2. **Presence strip**: `border-t border-border/60 px-3 py-1.5 text-xs muted` — `Eye size-3` + viewer names; steerer gets `Keyboard size-3` + ` (steering)` in foreground.
3. **Pinned "Latest changes"** collapsible: trigger `bg-muted/30 px-3 py-2 text-xs` — chevron, `Latest changes` font-medium, right-aligned mono `+123` emerald-400 ` -45` rose-400; expands to a `max-h-72` scrollable FileDiffList (§8).
4. **Steering composer** `border-t p-2`: hint line `text-[0.6875rem] muted` — `You're steering` / `Alice is steering — sending takes over`; then row: borderless Textarea `min-h-9 bg-muted/40` (bg-muted/70 while steering) placeholder `Message the agent…`, outline `Esc` button (`h-9 px-2.5 font-mono text-xs`, tooltip "Send Escape — interrupts what the agent is doing"), primary square send (`size-9`, ArrowUp). View-only fallback: `Watching — only workspace owners or the session owner can steer.`

**No running session** but desktop online: outline sm `Start coding on MacBook Pro` (MonitorUp icon) or `Start on my desktop ▾` device dropdown; sent state: spinner + `Start sent to MacBook Pro — waiting for the desktop…`.

**Kill dialog**: `sm:max-w-sm`; title `Kill this coding session?`; body "This force-terminates the terminal on <device> and ends the session. Uncommitted work in the worktree is kept, but Claude stops immediately."; ghost `Cancel` + destructive `Kill session`.

---

## 8. Diff View (`diff-view.tsx`) — Changes tab & Latest-changes

**FileNav summary card**: `rounded-md border`; header `bg-muted/30 px-3 py-1.5 text-xs`: `N files changed` font-medium + right mono `+adds`/`-dels`; below, click-to-scroll file list rows.

**Per-file card** (`rounded-md border`, collapsible):
- Sticky header `bg-muted/30 px-3 py-1.5 text-xs`: chevron (rotates open), status letter `w-3 font-mono font-semibold` — `A` emerald-400, `D` rose-400, `M` amber-400, `R`/`C` sky-400 — filename in mono (directory part muted, basename foreground), collapsed adds `N lines`, right-aligned `+X` emerald-400 / `-Y` rose-400 mono.
- Body: `font-mono text-[0.6875rem] leading-relaxed`, horizontally scrollable. Line grid `grid-cols-[3rem_3rem_1rem_1fr]`: old line #, new line # (right-aligned tabular, muted/50), +/− sign column (emerald-400 / rose-400), code text.
  - Added lines: row `bg-emerald-500/10`; deleted: `bg-rose-500/10`; context: plain.
  - Syntax highlighting via lowlight, GitHub-Dark-Dimmed-ish token colors from `--hljs-*` (keyword pink `oklch(.72 .16 350)`≈#e58fb1… strings green, numbers amber, titles periwinkle, comments gray italic).
  - Hunk headers `@@ -1,5 +1,7 @@`: `bg-indigo-500/5 text-indigo-300/80 px-3 py-0.5`.
- Files >300 lines start collapsed; expanded files cap at 500 lines with a full-width ghost `Show 500 more lines (N hidden)` button.

---

## 9. Notification Inbox (`inbox/inbox-view.tsx`)

Centered column `max-w-3xl px-4 py-4`, full height.

```
🔔 Inbox                                            Mark all read
┌──────────────────────────────────────────────────────────────┐
│ (◉)  EXP-42  Fix sync bug                        2h        ● │
│      Dennis commented on EXP-42                              │
└──────────────────────────────────────────────────────────────┘
┌──────────────────────────────────────────────────────────────┐  read rows at 60% opacity
│ (⇄)  EXP-40  Add dark mode                       1d          │
│      PR #128 was merged                                      │
└──────────────────────────────────────────────────────────────┘
```
- Header: `Bell h-4 w-4` + `Inbox` `text-lg font-semibold`; ghost sm `Mark all read` right (only if unread).
- Rows (one per issue group, `space-y-2`): card = `flex items-start gap-3 rounded-md border px-3 py-2`, hover `bg-accent/50`; fully-read groups `opacity-60`.
  - Left: `h-7 w-7 rounded-full bg-muted` circle with `h-3.5 w-3.5` muted type icon — assigned `UserPlus`, created `MessageSquarePlus`, comment/mention `MessageSquare`, status `CircleDot`, pr_opened `GitPullRequest`, pr_merged `GitMerge`, fallback `Bell`.
  - Line 1: identifier `font-mono text-xs muted`, issue title `text-sm truncate` (font-medium if unread), relative time right (`just now`/`37m`/`5h`/`2d`), unread dot `h-2 w-2 rounded-full bg-primary` (#e5e5e5).
  - Line 2: latest notification sentence `text-xs muted truncate`.
- Empty state: `CheckCircle2` icon, "All caught up", "Assignments, comments and mentions on issues you follow will show up here."

---

## 10. Global Search (`issue-search-sheet.tsx`)

Desktop: Dialog pinned at `top-[15%]` (not vertically centered), `sm:max-w-lg`, `max-h-[60vh]`, `p-0`.
- Header `px-3 py-3 border-b border-border/50`: `Search size-4` muted + borderless input placeholder `Search issues...` (`h-9 text-sm`).
- Idle: centered `Search size-8` @50% + "Type to search issues" (`p-12`, muted).
- Result rows: full-width ghost buttons `px-4 py-3` bottom-border `border-border/30`: status icon `size-4` + column [title `text-sm truncate`; below: project-color 1.5×1.5 dot + `Project name · EXP-42` `text-xs muted`].
- No hits: `No issues match "query"`.

---

## 11. Misc shared components

- **Buttons** (shadcn cva): base `rounded-md text-sm font-medium gap-2`; sizes: default `h-9 px-4`, `sm h-8 px-3 gap-1.5`, `xs h-6 px-2 text-xs gap-1` (svg 3×3), `icon size-9`, `icon-xs size-6`. Variants: default = `bg-primary` #e5e5e5 with dark text; outline = 1px border + `dark:bg-input/30`; ghost = transparent → hover `bg-accent/50`; destructive = `dark:bg-destructive/60` white text.
- **Badge**: `rounded-full px-2 py-0.5 text-xs font-medium gap-1`; outline variant = border-border + foreground text.
- **Avatar**: `rounded-full`, fallback `bg-muted` #262626 with initials (first letters of up to 2 words, uppercase).
- **Skeleton**: `bg-accent animate-pulse rounded-md`.
- **Duplicate banner** (detail page): `bg-accent/30 border-b px-4 py-2 text-sm` — `Files size-4` muted + "Duplicate of" + outline pill `h-5 rounded-full px-2 font-mono text-xs` `#EXP-12` + canonical title muted + right ghost `Undo2` `Unmark`.
- **Exponential logo**: 100×100 viewBox circle, masked by three swooping exponential-curve strokes (strokeWidth 3.5) sweeping from bottom-left up to the top-right; renders in `currentColor` (white in sidebar) at 28px.
- Dates: `Jul 12` (en-US short month + numeric day). Relative times: `just now`, `Nm`, `Nh`, `Nd`.

## 12. Copy strings quick reference
Sidebar: `Search`, `My Issues`, `Inbox`, `Reviews`, `Agents`, `Releases`, `Projects`, `No projects yet`, `Sign in to contribute`, `Workspace settings`, `New workspace`, `Account & notifications`, `Sign out`, `Admin`.
Board: `Issues`, `All Issues`, `Active`, `Backlog`, `Filter`, `New Issue`, `Clear all`, `No issues yet`, `Create an issue to start tracking work.`
Detail: `Details`, `Changes`, `Activity (N)`, `Leave a reply…`, `Add description...`, `Issue title`, `Delete issue`, `Confirm delete`, `Unmark duplicate`.
Dialog: `New issue`, `Create more`, `Create issue`, `Creating...`, `Make recurring…`, `Create recurring issue`.
Agent: `Coding now`, `Watch live`, `Live · <device>`, `Agent starting…`, `Kill session`, `Latest changes`, `Message the agent…`, `You're steering`, `Start coding on <device>`, `Jump to latest`.
Inbox: `Inbox`, `Mark all read`, `All caught up`.
