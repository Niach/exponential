# Exponential Desktop IDE — Pixel-Perfect Visual Spec

Source of truth for recreating the Rust/gpui desktop app in React/Remotion for the launch
video. Derived from `apps/desktop/crates/ui/src/*`, `apps/desktop/crates/theme/*`, and
`packages/design-tokens/tokens.json`. All values are exact unless marked "~".

---

## 1. Global visual system

### 1.1 Palette (forced dark — there is NO light mode)

| Token | Hex / rgba | Used for |
|---|---|---|
| `background` | `#0A0A0A` | window body, center pane, issue list, diff pane |
| `foreground` | `#FAFAFA` | primary text |
| `card` / `popover` / `sidebar` / `title_bar` / `tab_bar` | `#171717` | top bar, rail, tool-window sidebar, popovers/menus, collapsed terminal strip, tab-bar chrome |
| `primary` | `#E5E5E5` | primary buttons (near-white fill), unread dots, progress bars |
| `primary_foreground` | `#171717` | text on primary buttons |
| `secondary` / `muted` / `accent` | `#262626` | hover/active fills, pills, skeletons, inactive tab bg |
| `muted_foreground` | `#A1A1A1` | secondary text, icons, identifiers, timestamps |
| `border` / `sidebar_border` | `rgba(255,255,255,0.10)` | ALL 1px hairline borders |
| `input` | `rgba(255,255,255,0.15)` | input outlines, ghost-button hover mixes |
| `ring` | `#737373` | scrollbar thumb (70% opacity, 100% on hover), focus ring |
| `destructive`/`danger` | `#FF6467` | destructive buttons, error text |
| overlay | `rgba(0,0,0,0.5)` | dialog scrim |
| selection | `#3B82F6 @ 30%` | text selection (UI and terminal) |

**Fixed semantic accents** (status/priority/PR/due — never theme-derived):

| Token | Hex |
|---|---|
| neutral | `#A1A1AA` |
| yellow (in-progress, warning) | `#FACC15` |
| green (done, PR open, coding-now) | `#22C55E` |
| red (urgent, deletions, LIVE badge) | `#EF4444` |
| orange (high priority) | `#F97316` |
| blue (low priority, info, hunk headers) | `#3B82F6` |

**Solid indigo action button** (hand-rolled, the only saturated fill in the app):
`#4F46E5` base → hover `#4338CA` → pressed `#3730A3`, white text, 24px tall (`h-6`),
px 10, gap 4, radius 6, text 12px medium. Used by: board "New Issue" button + create-issue
dialog submit. Disabled = 50% opacity.

Terminal-only extra ANSI colors: magenta `#D946EF` (bright `#E879F9`), cyan `#06B6D4`
(bright `#22D3EE`).

Status group header tints (literal rgba, painted across the full header row):
- Todo: `rgba(212,212,216,0.08)`
- In Progress: `rgba(234,179,8,0.10)`
- Done: `rgba(34,197,94,0.10)`
- Backlog / Cancelled / Duplicate: `rgba(113,113,122,0.08)`

### 1.2 Typography

- **UI font: "Inter"** (bundled). Base size **13px** (one notch tighter than web's 14px —
  "compact density").
- Sizes actually used: `text_xs` ≈ 12px (metadata, identifiers, tool-window rows, sub-lines),
  `text_sm` ≈ 13–14px (row titles, group labels, dialog copy), 11px (`text-[11px]`) for the
  properties-panel UPPERCASE micro-labels and diff code, `text_xl` ≈ 20px **semibold** for
  the issue-detail title.
- Weights: regular 400 default; `MEDIUM` (500) for titles/labels/active tabs;
  `SEMIBOLD` (600) for section headers, release names, detail title; `BOLD` only for
  the M/A/D/R git status glyphs.
- **Mono font: "JetBrains Mono"** (bundled) — the terminal grid (13px, line-height 1.3)
  AND every issue identifier (`EXP-42`) in lists, inbox, search rows, duplicate picker,
  branch chips (`⎇ exp/EXP-42`), diff file paths/file lists.

### 1.3 Geometry / rhythm

- Radii: controls **6px** (`theme.radius`), dialogs/notifications **8px**, bulk bar **10px**,
  pills/dots/labels `rounded-full`.
- Control heights: lg 36 / md 32 / sm 24; xsmall ghost icon buttons ≈ 20–22px.
- Spacing scale (gpui = Tailwind×4px): gap-1=4, gap-1.5=6, gap-2=8, gap-3=12; px-2=8,
  px-3=12, px-4=16.
- Dots: 6px (`size-1.5`) colored dots everywhere (labels, projects, coding pill, flow lanes);
  8px (`size-2`) for project dots in group headers/inbox unread.
- Scrollbars: transparent track, `#737373 @ 70%` thumb.
- Icons: **Lucide** SVG set, stroke style, typically `xsmall` (~12–14px) or `small` (~16px).

### 1.4 Icon glossary (Lucide names)

Status: Backlog=`circle-dashed` (#A1A1A1), Todo=`circle` (#FAFAFA), In Progress=`timer`
(#FACC15), Done=`circle-check` (#22C55E), Cancelled=`circle-x` (#A1A1A1), Duplicate=`copy`
(#A1A1A1).
Priority: No priority=`minus` (#A1A1A1), Urgent=`triangle-alert` (#EF4444), High=`signal-high`
(#F97316), Medium=`signal-medium` (#FACC15), Low=`signal-low` (#3B82F6).
Other: search, inbox, circle-user, list-todo, git-pull-request, rocket, folder/folder-open,
git-merge, settings, plus, bell/bell-off, calendar-days, repeat, tag, square-terminal,
chevron-up/down/left/right, chevrons-up-down, ellipsis, external-link, play, square (stop),
circle-x, check, globe, megaphone, square-kanban, code, message-square, circle-dot, user-plus,
list-checks, pencil, undo-2, delete (trash), eye, file, sparkles, search-x.

---

## 2. Window anatomy

```
┌──────────────────────────────────────────────────────────────────────────────────┐
│ TOP BAR (38px, #171717, 1px bottom border)                                        │
│ [● Project name ⌄]                    …spacer…   [RunCfg ⌄][▶]  │  ⎇ main  ✓  ↑2 │
├────┬───────────────────────────────────────────────────────────────────────────--┤
│RAIL│ ┌ SIDEBAR (260px, #171717, right border) ┬ CENTER (#0A0A0A) ────────────────┐│
│44px│ │ tool-window header (30px)              │ [EXP-42 ✕][EXP-38 ✕] tab strip   ││
│    │ │ …tool window content (issue board,     │ ─────────────────────────────────││
│ 🔍 │ │  inbox, reviews, releases, files,      │  active screen: issue detail /   ││
│ ── │ │  branch flow graph)                    │  source control / file viewer /  ││
│ 📥 │ │                                        │  settings                        ││
│ 👤 │ │                                        │                                  ││
│ ☰  │ │                                        │                                  ││
│ ⇵  │ │                                        │                                  ││
│ 🚀 │ ├────────────────────────────────────────┴──────────────────────────────────┤│
│ ── │ │ TERMINAL DOCK (bottom, default 240px; collapsed = 29px strip)             ││
│ 📁 │ │ [claude · EXP-42 ✕][zsh ✕] [+]                                    [⌄]     ││
│ ⑂  │ │ ── JetBrains Mono 13px terminal grid on #0A0A0A ──                        ││
│    │ │ ● Process finished with exit code 0                                       ││
│ ⚙  │ └───────────────────────────────────────────────────────────────────────────┘│
│ 👤 │                                                                              │
└────┴──────────────────────────────────────────────────────────────────────────────┘
```

- The **rail** and **top bar** sit outside the dock area; the terminal dock spans the full
  width right of the rail, running *beneath* the sidebar column.
- Sidebar ↔ center is a draggable split (sidebar 180–520px, default 260).
- Everything sits on `#0A0A0A`; chrome surfaces (top bar/rail/sidebar/popovers/tab bars)
  are `#171717`; every separation is a 1px `rgba(255,255,255,0.1)` line.
- Optional **update banner** above everything: 34px strip, `info(#3B82F6) @ 14%` bg, ⓘ icon,
  13px text "Update available — Exponential 0.7.1 is out.", primary "Update" button, ✕.

---

## 3. Top bar (38px)

- bg `#171717`, bottom border, `px-8`, items gap 6.
- **Left — project picker**: leading glyph = project-type icon tinted with the project's
  color (dev=`code`, tasks=`square-kanban`, feedback=`megaphone`; fallback = 12px color
  dot), then project name (13px, medium, max-w 240 ellipsis), feedback boards append a muted
  `globe`, then a muted `chevrons-up-down` (xsmall). Dropdown (popover `#171717`, radius 8,
  max-h 320 scrollable): small muted "Projects" label header (or one label per workspace),
  project rows with check on the active one, separator, "New project…" with plus icon.
- **Right cluster** (right-aligned after a flex spacer):
  1. **Run bar**: ghost xsmall select labeled with the selected run-config name (placeholder
     "No run configurations" / "Loading…") with dropdown caret; menu lists configs +
     separator + "Edit configurations…". Next to it the **play button**: ghost xsmall,
     `play` icon in green `#22C55E` (disabled = muted); while running it becomes a `square`
     stop icon in red/danger. Only visible when a project is active.
  2. 1px × 16px vertical divider.
  3. **Git cluster** (ghost xsmall buttons, gap 4):
     - Branch chip: `⎇ main` (adds ` ●` suffix when the tree is dirty). Dropdown: "Branches"
       label, branch rows with check on current, separator, "Open changes view" (git-merge
       icon), "Check for updates".
     - Commit button: `check` icon only (tooltip "Commit…").
     - Status segment: spinner (xsmall, muted) while syncing; `Cloning repo… 42%` muted text
       during clone; red truncated error + ghost "Retry"; amber `⚠ 3 conflicts` chip
       (#FACC15 text) in conflict mode; sticky amber `triangle-alert` icon on background
       sync failure.
     - Context action: the count IS the button — muted `↓2` (fast-forward), `↑3` (push),
       `↓2 ↑1` (rebase+push), or a **primary** (near-white `#E5E5E5` fill, dark text)
       "Publish" button for unpublished branches. Clean & in-sync renders nothing.

---

## 4. Rail (44px wide, full height)

Vertical icon strip, bg `#171717`, right border, py-8, items gap 4, all ghost small icon
buttons (~24px). Top → bottom:

1. `search` (opens ⌘K sheet)
2. divider (24px wide, 1px, border color)
3. `inbox` — Inbox
4. `circle-user` — My Issues
5. `list-todo` — All Issues
6. `git-pull-request` — Reviews (6px amber-less **green?** no: plain dot badge, top-right,
   6px `#FACC15`-style — actually the Reviews dot uses `warning` amber `#FACC15` only for
   conflicts; Reviews badge is the same warning dot shown while open PRs exist)
7. `rocket` — Releases
8. divider
9. `folder` — Files
10. `git-merge` — Source Control (amber 6px badge top-right in conflict mode)
11. …flex spacer…
12. `settings` gear
13. `circle-user` — account button (very bottom). Dropdown: email label, "Settings",
    "Notifications", "Send Feedback" (thumbs-up), "New workspace" (plus), separator,
    "Sign out".

Active tool: icon tinted with the **active project's color** (fallback near-white primary),
button gets the selected (accent `#262626`) background, plus a **2px rounded accent bar**
hugging the rail's left edge spanning the icon height — JetBrains-style selection marker.

---

## 5. Sidebar tool windows (the 260px column)

Shared chrome: **tool header**, 30px tall, px-12, bottom border, icon (xsmall) + title
(12px, medium) at 70% sidebar-foreground opacity, optional right-aligned ghost xsmall
action icons (e.g. "+" on Releases, refresh `repeat` on Files/Source Control, `list-checks`
"Mark all read" on Inbox).

### 5.1 All Issues / My Issues (the board)

Filter bar (px-16):
- Title row (py-8): "All Issues" / "My Issues" (13px medium) left; right: filter popover
  trigger (`list-filter` ghost button) + the **indigo "New Issue" button** (plus icon +
  label; project scope only).
- Tabs row (pb-4, gap 4, left-aligned): rounded-full pills 24px tall, px-12, 12px text —
  `All Issues` · `Active` · `Backlog`. Active pill = `#262626` bg + `#FAFAFA` text medium;
  inactive = transparent, `#A1A1A1` text, hover fills accent.
- Active filter pills row appears below when filters are set (removable ✕ pills).
- Below: the virtualized issue list (see §6).

### 5.2 Inbox

Rows (px-8, py-6, radius 6, hover accent@30%, selected accent@60%):
```
(●) EXP-42  Fix login redirect …            2h ●
    Danny merged the pull request for EXP-42…
```
- Leading 24px circular badge (`#262626` fill) with the latest notification's type icon in
  muted: assigned=`user-plus`, comment/mention=`message-square`, status=`circle-dot`,
  pr_opened=`git-pull-request`, pr_merged=`git-merge`, fallback `bell`.
- Line 1: mono identifier (12px muted) + issue title (12px; unread = medium `#FAFAFA`,
  read = muted). Line 2: the full sentence, muted 12px, truncated.
- Right: relative time ("just now"/"5m"/"2h"/"3d", muted 12px) + 8px unread dot in
  near-white `#E5E5E5`.
- Empty state: "All caught up." (12px muted, p-12).

### 5.3 Reviews (open PRs)

Grouped by project — group header: 8px project-color dot + project name (12px semibold
muted). Rows (two lines, radius 6, hover accent@30%):
```
⑂ EXP-51  Add release progress bar   [Merge]
   #12 · exp/EXP-51
```
- `git-pull-request` icon in **green #22C55E**, mono identifier muted, title 12px `#FAFAFA`,
  trailing **outline xsmall button** "Merge" → armed state "Confirm merge" (danger red) →
  "Merging…" with spinner. Sub-line `#N · branch` muted, pl-20. Optional red error caption.
- Empty: "No open pull requests."

### 5.4 Releases

List rows (two lines):
```
🚀 v1.0 – Launch                    (coding) (Shipped)
   Target Jul 15 · 3 of 8 done
```
- `rocket` icon: green when shipped, muted otherwise. Name 12px `#FAFAFA`.
- Pills (rounded-full, 1px border `green @ 40%`, 12px text): "Shipped" (green text);
  "coding" pill = 6px green dot + word "coding".
- Sub-line: `Target <Mon D> · N of M done` muted.
- Header has a "+" (New release).

**Release detail (drill-down in the same panel):**
- Header row 30px: `‹` back chevron, release name (12px medium, truncated), then ghost
  xsmall actions: `+ Add issues`, `▶ Start coding` (play icon green) — or `⨯ Stop` (red)
  while running — and `…` menu ("Mark shipped"/"Unship" with circle-check, separator,
  "Delete release" ▸ "Confirm delete").
- Summary block (px-12, bottom border): rocket + name (13px semibold); one-line description
  teaser (muted); meta chip row (Shipped pill / `calendar-days` "Target Jul 15" / coding
  pill / outline PR chip "PR #7 · open" with green PR icon, merged = muted git-merge);
  then a **progress bar**: 4px tall rounded-full track `muted @ 20%`, green `#22C55E` fill,
  right label "3 of 8 done" (12px muted).
- Below: the release's issues grouped by status (same issue list core, no bulk bar).

### 5.5 Files

Trunk file tree, JetBrains-style rows 24px: chevron (dirs) / 14px spacer (files),
`folder`/`folder-open`/`file` icon muted xsmall, name 13px (`#FAFAFA`; gitignored at 60%
muted), trailing git-status letter (M yellow, A green, D red, R blue, ? muted, semibold
12px). Active (open in viewer) row gets the solid accent bg. Indent per depth.

### 5.6 Source Control (branch flow graph)

A connected tree drawn with 1px rails (`muted @ 35%`) in 14px gutter columns, 24px rows:
```
main ✓
 ├ ● exp/rel-v1-0
 │   ├ ✔ exp/EXP-40        ↑2
 │   └ ⏱ exp/EXP-41   worktree
 └ ● exp/EXP-38            ↑1 ↓3
```
- Indicator slot 12px: merged = green `circle-check`; local work in progress = yellow
  `timer`; PR open = 6px green dot; PR closed = red dot; idle = muted dot; default branch =
  none + label medium weight.
- Label 12px (`#FAFAFA`; "other" lanes muted), trailing muted `↑N ↓M` counts, muted
  "worktree" tag, `check` on the checked-out branch, hover-revealed trash delete button.
- Row highlight: viewing = accent @ 40%; hover = accent @ 25%.
- Clicking a branch lane VIEWS its history in the center; issue lanes open that issue's
  Changes tab.

---

## 6. Issue list (center of the board tool window)

- Group header, 28px: status-tinted full-width bar (see §1.1 tints), bottom border
  `border @ 50%` — chevron collapse button (ghost xsmall), status icon (colored, xsmall),
  status label 13px medium ("Backlog", "Todo", "In Progress", "Done", "Cancelled",
  "Duplicate"), count in muted 12px.
- Issue row, **28px tall**, px-12, bottom border `border @ 30%`, hover = accent @ 30%
  (`list_hover`), selected = solid accent. Cell order:

```
[☐] [prio] [EXP-42 ] [status] [Title text……………] [○ label] [◔] [📅 Jul 3]
 20   24     72(mono)   24        1fr truncate       auto     28    auto
```

  1. Bulk checkbox (20px, hover-revealed; pinned while any selection exists).
  2. Priority dropdown: ghost xsmall button with the colored priority glyph.
  3. Identifier: 72px, **JetBrains Mono 12px muted**, ellipsis.
  4. Status dropdown: colored status glyph button.
  5. Title: 13px, truncating; recurring issues prefix a muted `repeat` glyph.
  6. Label chips: rounded-full, 1px `border @ 50%`, px-6, 12px muted text, 6px color dot.
  7. Assignee: avatar (initials, xsmall ~16px) or a 16px dashed-border circle with a tiny
     muted `user` glyph at 50%.
  8. Due date: muted `calendar-days` + "Jul 3" (12px muted); unset = icon only at 30%.
- Dropdown option rows: colored icon + label + check mark on the RIGHT.
- Right-click context menu (popover #171717): label header = identifier, "Open issue"
  (pencil), "Mark as done"/"Move to todo", "Copy issue ID", "Remove from release",
  "Unmark duplicate", separator, submenus Status / Assignee / Priority / Labels /
  "Add to release" / "Set due date" (Tomorrow · End of this week · In one week · Clear due
  date), separator, "Delete issue" ▸ "Confirm delete".
- **Bulk action bar**: floating, centered, 16px above bottom — popover bg `#171717`, 1px
  border, radius 10, shadow-lg, px-8 py-4, gap 4: "3 selected" (12px medium) · ✕ · status ·
  priority (signal-high) · assignee · labels (tag) · add-to-release (rocket) · delete (red
  trash, nested "Confirm delete 3 issues"). All icon-only ghost xsmall with tooltips.
- Empty states (centered icon 24px muted + 13px medium title + 12px muted line):
  "No issues yet / Create an issue to start tracking work.",
  "No issues match your filters", "No issues in this release".
- Skeletons while syncing: one header + five rows of rounded gray (`#262626`) blocks.

---

## 7. Center screens panel

- **Tab strip** (compact TabBar, bg `#171717`): one tab per open issue/file, singleton
  Source Control/Settings tabs. Tab label = issue identifier or file name; active tab =
  accent bg + `#FAFAFA` text; hover reveals `external-link` (undock) and `✕` buttons.
- Content on `#0A0A0A`.
- Empty state: `inbox` icon 24px muted, "Nothing open" (13px medium), "Pick an issue from
  the sidebar — it opens as a tab here." (12px muted). No-projects variant: `folder` icon,
  "No projects yet", body copy, primary "New project…" button.

---

## 8. Issue detail (the hero screen)

Layout: ONE header row → optional duplicate banner → two-pane body (scrollable left column
+ fixed 288px properties panel) OR full-width Changes tab.

### 8.1 Header row (px-16, py-6, bottom border, 12px text)

```
Details  Changes            3 / 17 ˄ ˅   ▶ Start coding   ● Coding now · MacBook   🔔 Subscribed   …
```
- Left: `Details` · `Changes` segment buttons (ghost xsmall; active = `#FAFAFA`, inactive
  muted).
- Right cluster: EXP-48 switcher "3 / 17" (muted) + chevron-up/down ghost buttons
  (tooltips "Previous issue (K)" / "Next issue (J)");
  **Start coding control**: ghost xsmall `play` icon in green + "Start coding" label
  (disabled = muted icon + tooltip reason: "Checking local tools…", "Link a repository to
  this project in workspace settings."). While a local session runs it swaps to:
  optional `● LIVE — public` (red dot + red 12px medium text), `● Coding…` (6px green dot +
  muted text), and a "Stop" button (circle-x in red).
  **Coding-now pill** (when another device codes): rounded-full 1px `green @ 40%` border,
  6px green dot, "Danny coding now · MacBook Pro" 12px.
  **Subscribe toggle**: ghost xsmall `bell`/`bell-off` muted icon + "Subscribed"/"Subscribe".
  `…` menu (duplicates only): "Unmark duplicate".

### 8.2 Duplicate banner
Full-width strip, accent @ 30% bg, bottom border, px-16 py-8: muted `copy` icon,
"Duplicate of", outline xsmall chip `#EXP-12`, canonical title (muted, truncating),
right ghost "Unmark" with undo-2 icon.

### 8.3 Left column (centered, max-width 768px)

- **Title**: borderless input, ~20px semibold, px-16, placeholder "Issue title".
- **Description**: block-based GFM markdown editor (headings H1–H3, bold/italic/strike,
  inline code, bullet/ordered/task lists, blockquote, code blocks, links, images,
  @mentions as name pills, #EXP-42 issue pills). Placeholder "Add description..."
  (muted @ 60%).
- **Attachments row**: horizontal thumbnail rail (when images exist).
- **Timeline** ("Activity (N)") below a **full-bleed top border**; content re-centers to
  768px, px-16 py-12:
  - Header "Activity (7)" 12px medium muted.
  - **Event rows** (one line, py-4, 12px muted): muted type icon + **actor name**
    (medium, `#FAFAFA`) + phrase — exact strings: "changed status from todo to in
    progress", "assigned Ada", "removed the assignee", "added label bug", "opened pull
    request #42" (link-colored, clickable), "merged pull request #42", "added this to
    release v1.0", "moved this to another project (EXP-4 → WEB-9)".
  - **Comment rows**: avatar (initials) + author name (medium) + relative time
    ("3 minutes ago") + hover `…` menu (Edit/Delete for author/admin); body rendered as
    markdown.
  - **Composer**: mention-capable multi-line input, placeholder "Leave a reply…",
    auto-grows 2–8 lines; Cmd/Ctrl+Enter submits; Send button disabled while empty.

### 8.4 Properties panel (right, 288px, left border, px-12 py-12, gap 12)

Each group: 11px UPPERCASE medium muted micro-label over the control:
`STATUS`, `PRIORITY`, `ASSIGNEE`, `LABELS`, `RELEASE`, `DUE DATE` (+ recurrence control
underneath: `repeat` trigger, "Every 2 weeks", popover with calendar + interval/unit
selects + "Stop recurring"), `PROJECT` (8px color dot + name chip). Controls are ghost
buttons showing the colored option glyph + label; menus identical to row dropdowns.
Due date control: "Due date" when empty; `calendar-days` + "Jul 3" when set; popover holds
a calendar + Clear.

### 8.5 Changes tab (full-width, replaces both panes)

Header (px-16 py-8, bottom border):
```
⎇ exp/EXP-42   ● Claude running   Local — includes uncommitted   5 files +120 −34      [⌸ Open terminal in worktree] […]
```
- Branch in mono 12px `#FAFAFA`; "Claude running" = 6px green dot + muted text; source
  label muted ("Local — includes uncommitted" / "PR #12"); stats: "5 files" muted,
  `+120` green, `−34` red.
- Right: ghost "Open terminal in worktree" (square-terminal icon), `…` menu:
  "Update from main" (repeat icon), "Clean up worktree" (trash; disabled variant reads
  "Clean up worktree — stop the running session first").
- Body: **240px file list** (left, right border) — rows: mono status letter (M yellow /
  A green / D red / R blue) + mono filename 12px, hover accent@30, click scrolls the diff —
  and the **side-by-side diff** filling the rest.
- Empty tier: "No local worktree or pull request yet." + "Start coding on this issue, or
  open it in the web app."

---

## 9. Diff renderer (shared by Changes + Source Control)

- Code: **mono 11px**, line rows **18px**, on `#0A0A0A`.
- **File header** (26px, `muted @ 30%` bar, 1px border, mono 12px): path (`#FAFAFA`;
  renames show `old → new` with the old muted), non-modified status badge (accent @ 50%
  chip: "added"/"removed"/"renamed"), right-aligned `+12` (green, lightened) `-4` (red).
- **Hunk header** (18px): `@@ -10,7 +10,9 @@` in blue-tinted mono — bg `#3B82F6 @ 5%`,
  text lightened blue @ 80%.
- **Line rows**: 50/50 split (old left / new right), 1px border divider between columns.
  Each side: 40px right-aligned line-number gutter (colored to match the row kind @ 80%),
  then code. Added = bg `#22C55E @ 10%`; removed = bg `#EF4444 @ 10%`; context =
  transparent; filler (unpaired side) = `muted @ 30%` blank. Tree-sitter syntax
  highlighting on top.
- Notes: "No textual diff (binary or too large)." / "Renamed." (muted, 24px row).
- 8px gap between files. Loading: "Loading changes…"; error in red.

---

## 10. Source Control screen (center)

```
┌ conflict banner (when a rebase/merge is paused) ──────────────────────────────┐
├──────────360px──────────────┬──────────────────────────────────────────────────┤
│ [stash strip]               │                                                  │
│ Staged (2)                  │        side-by-side diff of the                  │
│  ☑ M src/app.rs             │        selected file / commit                    │
│ Changes (3)                 │                                                  │
│  ☐ ? new_file.rs            │   (or: "Select a file or commit                  │
│ ─────────────────────────── │        to view its diff.")                       │
│ [commit message input]      │                                                  │
│ [Commit] [Commit & Push]    │                                                  │
│ ─────────────────────────── │                                                  │
│ History                     │                                                  │
│  Fix login redirect         │                                                  │
│  danny · 2 hours ago        │                                                  │
│  [Load more]                │                                                  │
└─────────────────────────────┴──────────────────────────────────────────────────┘
```
- Left column 360px, right border. Group headers 12px semibold muted ("Staged (2)",
  "Changes (3)", "History").
- Change rows: checkbox + bold colored status letter (14px slot) + path 12px, radius 6,
  selected accent@60.
- Commit box: small input + "Commit" (default) and "Commit & Push" (**primary** near-white)
  buttons; one-time identity prompt card ("Set the author for commits in this repository:"
  + name/email inputs + "Save & commit").
- Commit rows: subject 12px `#FAFAFA` + `author · 2 hours ago` muted.
- Stash strip (accent@20 bg): "Stashed changes from a branch switch" + Restore / Discard.
- **Conflict banner**: `#FACC15 @ 12%` bg, semibold title "Rebase paused — 3 conflicted
  files", wrap of amber chips `⚠ src/main.rs` (warning @ 20% fill), then buttons:
  **"Fix conflicts with Claude"** (primary), "Open terminal", "Abort rebase" (danger).
- Viewing-another-branch banner: muted@30 strip, `⎇ branch-name` + ghost "Back to current".

---

## 11. Terminal dock & the embedded Claude coding session

- Bottom dock, default 240px; **collapsed = 29px strip**: `#171717`, top border,
  `square-terminal` icon + "Terminal (2)" 12px muted, right `chevron-up`.
- Expanded: compact tab strip (bg #171717) — tab labels: **`claude · EXP-42`** (coding
  session), **`claude · release v1.0`** (release orchestrator), run-config name (run tab),
  shell basename (`zsh`); live OSC titles can override. Tab suffix: exit-code badge
  (12px, px-4, radius 3 — green@15 bg + green text for 0, red for non-zero),
  hover-revealed `external-link` (undock) + `✕`. A `+` sits right after the last tab;
  far-right `chevron-down` collapses.
- **Terminal grid**: JetBrains Mono 13px, line-height 1.3 (17px rows), padding 4px x /
  2px y, bg `#0A0A0A`, fg `#FAFAFA`, cursor = solid `#FAFAFA` block (inverted glyph),
  selection blue@30. ANSI: black `#262626`, red `#EF4444`, green `#22C55E`, yellow
  `#FACC15`, blue `#3B82F6`, magenta `#D946EF`, cyan `#06B6D4`, white `#E5E5E5`;
  brights lightened (+0.08 L), bright black `#737373`, bright white `#FAFAFA`.
- A running Claude session is literally the `claude` CLI TUI in this grid (spinner,
  tool-call lines, ANSI colors) — seeded with a plan-first prompt; Claude commits, pushes,
  and opens the PR itself.
- **Exit strip** under a dead tab: top border, px-12 py-4, 6px dot (green 0 / red non-0) +
  "Process finished with exit code 0" 12px muted.
- **Remote steering banner** (amber@12 bg, below the tab strip): `eye` icon amber +
  "Remote steering — dennis@…" + outline "Take over" button.
- Empty-but-expanded dock auto-spawns a shell (no empty state). All-tabs-undocked hint:
  "All terminal tabs are open in separate windows."

---

## 12. Dialogs (all: #171717 popover surface, radius 8, black@50 scrim, title bar + ✕)

### 12.1 Start coding — issue variant (420px, title "Start coding on EXP-42")
- Muted intro: "Claude works on EXP-42 in its own worktree and opens the pull request when
  done."
- Two labeled selects side-by-side: **Model** (Fable / Opus / Sonnet — default Fable) and
  **Effort** (CLI default / Low / Medium / High / XHigh / Max).
- "Plan mode" checkbox row.
- On live-public boards: "Keep private" checkbox + muted note ("This project streams coding
  sessions publicly. Check to keep this session out of the public view.").
- Footer right: primary **"Start coding"** button ("Starting…" + spinner while launching).

### 12.2 Start coding — release variant (560px, max-h 85%, title "Start coding on release")
- Intro: "One Claude orchestrator implements the checked issues of “v1.0” — one subagent
  per issue."
- Scrollable (240px) issue **checklist grouped by repository** (group header = repo
  `owner/name`); rows = checkbox + mono identifier + title; excluded rows greyed with
  reason ("no repository linked"); pre-unchecked closed issues show a muted state hint.
- Model + Effort row; **Subagent model / Subagent effort** row (both with "Inherit").
- Toggle rows: "Dynamic workflows (ultracode)" switch + muted caption ("Runs the
  orchestrator with --effort ultracode — works with any model."); "Plan mode".
- Warnings (12px amber): "One repository per run — deselect the others." / "At most 30
  issues per run…" / "Large releases spawn many subagents — this can be token-expensive."
- Footer: primary "Start coding".

### 12.3 New release (480px, title "New release")
Name input ("Release name (optional)"), search input ("Search issues…"), 300px scrollable
checkbox list of open issues, footer: outline "Cancel" + primary "Create with 3 issues"
(disabled at 0).

### 12.4 Create issue
Header: project pill (color dot + prefix) › "New issue" › ✕. Borderless large title input;
markdown editor body (image paste); chip row: status/priority/assignee/labels/due-date +
`…` (Make recurring…); footer: "Create more" switch + **indigo submit button**.

### 12.5 ⌘K search sheet (512px, ~15% from top)
Borderless search input on top; sections **Issues** / **Files** / **In files**
(26px section headers; 44px two-line rows): issues = status icon + title + `project · EXP-42`
sub-line with project dot; files = `file` icon + name + directory; content = matched line +
`path:line`. ↑/↓ keyboard selection highlight = solid accent.

### 12.6 Mark as duplicate (480px)
Search input ("Search the canonical issue…") + 320px result list (mono identifier + title
rows).

---

## 13. Motion & states worth animating in the video

- Electric-sync-first UX: every mutation lands via server echo — rows/pills/badges update
  "by themselves" (great for split-screen web↔desktop sync shots).
- Hover-reveal affordances: row checkboxes, tab ✕/undock, flow-lane delete.
- Two-click confirms: Merge → "Confirm merge" (turns red); Delete ▸ "Confirm delete".
- Terminal dock expanding when "Start coding" spawns the `claude · EXP-42` tab.
- Green "Coding now" pill appearing on other clients while the session runs.
- Release progress bar filling as subagent PRs merge; "Shipped" pill flipping green.
- Skeleton rows → content on first sync.

## 14. Exact label strings glossary (verbatim)

"All Issues", "Active", "Backlog", "New Issue", "My Issues", "Inbox", "Reviews",
"Releases", "Files", "Source Control", "Search", "Settings", "Mark all read",
"All caught up.", "No open pull requests.", "Merge", "Confirm merge", "Merging…",
"No releases yet.", "New release", "Add issues", "Start coding", "Stop", "Mark shipped",
"Unship", "Delete release", "Confirm delete", "Shipped", "coding", "Target Jul 15",
"N of M done", "No issues", "Details", "Changes", "Subscribe", "Subscribed",
"Duplicate of", "Unmark", "Add description...", "Leave a reply…", "Activity (N)",
"No activity yet. Be the first to add a comment.", "Start coding on EXP-42",
"Start coding on release", "Starting…", "Plan mode", "Keep private", "Model", "Effort",
"Subagent model", "Subagent effort", "Dynamic workflows (ultracode)",
"Local — includes uncommitted", "Claude running", "Open terminal in worktree",
"Update from main", "Clean up worktree", "Staged (N)", "Changes (N)", "History",
"Commit", "Commit & Push", "Load more", "Fix conflicts with Claude", "Open terminal",
"Abort rebase", "Rebase paused — N conflicted files", "Back to current", "Terminal (N)",
"Process finished with exit code 0", "Remote steering — <user>", "Take over",
"LIVE — public", "Coding…", "Coding now", "New shell", "No terminal sessions",
"Nothing open", "Pick an issue from the sidebar — it opens as a tab here.",
"No projects yet", "New project…", "Publish", "Check for updates", "Open changes view",
"Branches", "Projects", "Sign out", "Send Feedback", "New workspace",
"Update available — Exponential X is out.", "Restart to update", "Tomorrow",
"End of this week", "In one week", "Clear due date", "Copy issue ID", "Mark as done",
"Move to todo", "Open issue", "Remove from release", "Add to release", "Set due date",
"Delete issue", "Unassign", "No priority", "Urgent", "High", "Medium", "Low",
"Todo", "In Progress", "Done", "Cancelled", "Duplicate", "Every 2 weeks", "Stop recurring".
