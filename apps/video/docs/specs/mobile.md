# Exponential — Mobile UI Recreation Spec (iOS + Android)

Source-of-truth archaeology from:
- iOS: `apps/ios/Exponential/UI/**`, `apps/ios/ExpUI/Sources/GlassTheme.swift`, `DesignTokens.generated.swift`
- Android: `apps/android/app/src/main/java/com/exponential/app/ui/**`, `ui/theme/{Glass.kt,Theme.kt,StatusColors.kt,DesignTokens.generated.kt}`
- Shared: `packages/design-tokens/tokens.json`

**Headline:** both apps are deliberately the SAME design — a dark-only "glass" (Linear-style)
look: translucent frosted rows with hairline white strokes floating over a vertical
zinc gradient, a floating capsule bottom-nav pill with a detached circular compose
button, and Linear-style issue rows (priority glyph → mono identifier → status glyph →
title → label dots → due date → avatar). Android is an explicit 1:1 port of the iOS
GlassTheme (its `Glass.kt` says so). One shared design recreates both; platform
deltas are listed at the end.

---

## 1. Global visual language

### 1.1 Background
Every screen floats on a full-bleed vertical gradient (NOT the flat web `#0A0A0A`):

- Top: **zinc-950 `#09090B`**
- Bottom: **zinc-900 `#18181B`**
- Direction: top → bottom, linear.
- (iOS's Zinc constants round to `#0F0F12`→`#1A1A1C`; Android uses exact `#09090B`→`#18181B`.
  For recreation use the Tailwind zinc values `#09090B`→`#18181B`.)

### 1.2 Glass surfaces (the signature material)
All fills are translucent white over the gradient (iOS uses real `.ultraThinMaterial`
blur; Android approximates with plain low-alpha white fills — no blur):

| Token          | Fill                     | Stroke                    | Radius |
|----------------|--------------------------|---------------------------|--------|
| glassRow       | `rgba(255,255,255,0.05)` | `rgba(255,255,255,0.06)`  | 10px   |
| glassRow active| `rgba(255,255,255,0.15)` | `rgba(255,255,255,0.20)`  | 10px   |
| glassSection   | `rgba(255,255,255,0.04)` | `rgba(255,255,255,0.08)`  | 12px   |
| glassCard      | `rgba(255,255,255,0.06)` | `rgba(255,255,255,0.10)`  | 16px   |
| glassButton    | same as glassRow/active  | same                      | full capsule (50%) |

- Stroke width is a **hairline: 0.5px** everywhere.
- glassCard additionally gets `shadow: rgba(0,0,0,0.3), blur 20, y 8` (iOS only).
- Standard glass-row content padding: **12px horizontal, 10px vertical**.

### 1.3 Text emphasis tiers (alpha over white)
- Primary: `rgba(255,255,255,1.0)`
- Secondary: `0.7`
- Tertiary: `0.5`
- Quaternary: `0.3`

### 1.4 Accent + semantic colors (exact hex, shared tokens)
- **Accent indigo `#6366F1`** — count badges, selection checkmarks, inbox unread dot (iOS).
- Neutral `#A1A1AA` (zinc-400)
- Yellow `#FACC15`
- Green `#22C55E`
- Red `#EF4444`
- Orange `#F97316`
- Blue `#3B82F6`
- Destructive (buttons/errors): `#FF6467`

Web-derived flat palette (used for M3 roles on Android, sheets, etc.):
background `#0A0A0A`, foreground `#FAFAFA`, card/popover `#171717`,
primary `#E5E5E5`, primary-foreground `#171717`, secondary/muted/accent `#262626`,
muted-foreground `#A1A1A1`, border `rgba(255,255,255,0.10)`, input `rgba(255,255,255,0.15)`,
ring `#737373`.

### 1.5 Typography
- **iOS: SF Pro (system)** — sizes at default Dynamic Type: largeTitle 34, title2 22,
  title3 20, headline 17 semibold, body 17, subheadline 15, caption 12, caption2 11.
- **Android: Roboto (system default)** — M3 scale as themed: headlineLarge 32sp/semibold,
  titleSmall 14sp/medium, labelLarge 14sp/medium, labelMedium 12sp/medium,
  labelSmall 11sp/medium, bodyMedium 14sp/regular.
- Issue **identifiers are monospaced** (SF Mono / Roboto Mono), ~11–12pt, tertiary white 50%.
- No Inter on mobile (Inter is web/desktop only). For a Remotion recreation, SF Pro /
  Inter at these sizes reads correctly; use a mono font for identifiers.

### 1.6 Radii & geometry quick sheet
- Row radius 10 / section 12 / card 16 / pills+bars full capsule.
- Hairline strokes 0.5px. Screen gutter 16px. Row v-gap 3px.
- Issue row height ≈ **42px** (10px v-padding ×2 + 22px content line).
- Circular icon buttons: 38px. Compose FAB: 52px. Tab hit target: 44×42 (iOS) / 48×42 (Android).

---

## 2. Status & priority iconography (exact)

Colors identical on both platforms (one exception noted):

| Status       | Label (iOS / Android)        | Color                       | iOS SF Symbol            | Android Material icon  |
|--------------|------------------------------|-----------------------------|--------------------------|------------------------|
| backlog      | "Backlog"                    | `#A1A1AA`                   | `circle.dashed` (dashed ring) | `RadioButtonUnchecked` (ring) |
| todo         | "Todo"                       | iOS: near-white `#FAFAFC`; Android: `#A1A1AA` | `circle` (ring)          | `RadioButtonUnchecked` |
| in_progress  | "In Progress" / "In progress"| `#FACC15` yellow            | `hourglass`              | `HourglassTop`         |
| done         | "Done"                       | `#22C55E` green             | `checkmark.circle.fill`  | `CheckCircle` (filled) |
| cancelled    | "Cancelled"                  | `#EF4444` red               | `xmark.circle.fill`      | `Cancel` (filled ⊗)    |
| duplicate    | "Duplicate"                  | `#A1A1AA`                   | `doc.on.doc`             | `ContentCopy`          |

| Priority | Label        | Color            | iOS SF Symbol                  | Android icon        |
|----------|--------------|------------------|--------------------------------|---------------------|
| none     | "No priority"| `#A1A1AA`        | `minus`                        | `Remove` (–)        |
| urgent   | "Urgent"     | `#EF4444` red    | `exclamationmark.triangle.fill`| `Warning` (⚠ filled)|
| high     | "High"       | `#F97316` orange | `chevron.up`                   | `KeyboardArrowUp`   |
| medium   | "Medium"     | `#FACC15` yellow | `equal` (=)                    | `DragHandle` (≡)    |
| low      | "Low"        | `#3B82F6` blue   | `chevron.down`                 | `KeyboardArrowDown` |

Status group display order: **In Progress, Todo, Backlog, Done, Cancelled, Duplicate**
(empty groups hidden). Priority order: Urgent, High, Medium, Low, None.

Due-date color: overdue → `#EF4444`, due today → `#F97316`, else white 50%.
Due-date text: "Today", "Tomorrow", else "MMM d" (e.g. "Jul 18").

---

## 3. HERO SCREEN — Issue list (project home / Issues tab root)

### 3.1 Full-screen ASCII layout (portrait phone)

```
┌────────────────────────────────────────────────────────┐  ← zinc gradient #09090B→#18181B
│ status bar (light content)                             │
│                                                        │
│ (Exponential ⌄)                              (⚙)      │  ← pinned nav row
│                                                        │
│ Exponential                                            │  ← Android only: 32sp bold title,
│                                                        │    scrolls with content
│ (≡̶ )  (All issues) (Active) (Backlog)                 │  ← filter bar: circle btn + tab pills
│                                                        │
│ ⌄ ⏳ In Progress  3                                    │  ← status group header
│ ┌────────────────────────────────────────────────────┐ │
│ │ ⚠  EXP-42   ⏳  Fix Electric sync loop  ●● 📅 Today ⓐ│ │  ← glass row, r=10
│ └────────────────────────────────────────────────────┘ │  ← 3px gap
│ ┌────────────────────────────────────────────────────┐ │
│ │ ↑  EXP-38   ⏳  Canonical issue ordering    📅 Jul 18│ │
│ └────────────────────────────────────────────────────┘ │
│                                                        │
│ ⌄ ○ Todo  5                                            │
│ ┌────────────────────────────────────────────────────┐ │
│ │ =  EXP-51   ○  Add release picker to detail      ⓐ │ │
│ └────────────────────────────────────────────────────┘ │
│ │ –  EXP-49   ○  Polish onboarding copy               │ │
│ ┆ …                                                  ┆ │
│                                                        │
│ ⌄ ✓ Done  12                                           │
│ │ –  EXP-31   ✓  Ship v7 public boards                │ │
│                                                        │
│  ╭───────────────────────────────╮          ╭────╮     │
│  │ ☰   🔍   🤖   📥   📦        │          │ ✎  │     │  ← floating pill + compose FAB
│  ╰───────────────────────────────╯          ╰────╯     │
│ home indicator                                         │
└────────────────────────────────────────────────────────┘
```

### 3.2 Pinned top nav row
- Transparent over the gradient (iOS: `.ultraThinMaterial` nav-bar background appears
  when content scrolls under; Android: plain transparent row, padding 12h/6v).
- **Left/center: project switcher control** — one tappable glass capsule:
  - iOS: nav-bar principal slot, project name in `.headline` (17pt semibold, white)
    + `chevron.up.chevron.down` glyph (11pt semibold, white 50%), 5px gap. No capsule fill on iOS.
  - Android: glass capsule pill (fill white 5%, stroke white 6%, radius full), padding
    14h/8v; name in titleSmall 14sp medium white; trailing `UnfoldMore` (up/down carets)
    16dp white 70%; 6dp gap. Sits top-LEFT.
- **Right: settings gear** — iOS `gearshape` 17pt white 70%, plain; Android circular
  glass button 38dp (fill white 5%, hairline stroke white 6%) with `Settings` icon 20dp white 70%.
- Tapping the switcher opens the **project switcher bottom sheet** (§6).

### 3.3 Large title (Android only, iOS keeps inline bar)
- First LazyColumn item: project name, 32sp semibold (headlineLarge), white,
  padding top 4 / bottom 12. Scrolls away with content.

### 3.4 Filter bar (both platforms, same design)
Row at 16px gutter, 8px vertical padding, 8px gaps:
1. **Circular filter button** 38px: glass circle, icon = iOS `line.3.horizontal.decrease`
   (3 shrinking lines) / Android `FilterList`; icon white 70% (white 100% when filters active;
   capsule turns active fill 15% + stroke 20%).
   - Active-count badge: 15px circle, **indigo `#6366F1`**, white 10pt semibold number,
     offset to the top-right corner.
2. **Three tab pills** — exact labels: **"All issues"**, **"Active"**, **"Backlog"**.
   - Capsule glass pills, padding 14h/8v.
   - Text 15pt/14sp medium; selected = white 100% + active fill (white 15%)/stroke (white 20%);
     unselected = white 70% + rest fill (white 5%)/stroke (white 6%).
3. Android adds a "Clear ✕" pill after tabs when filters are active; iOS instead puts
   "Clear all" at the end of the active-filter-pills row below.

**Active filter pills row** (when filters set): small capsules (padding 10h/6v),
caption 12pt white 70%, leading colored status/priority glyph (11pt) or a 7px label
color dot, trailing ✕ (8pt semibold white 50%). Last pill: "Clear all".

### 3.5 Status group header
Height ≈ 32px; padding 8h/6–8v; tappable to collapse.
```
[⌄ 12px white50]  [status glyph 12–14px, semantic color]  [Label 15pt/14sp medium white70]  [count 12pt white50]
```
- Collapse chevron: down when expanded, right when collapsed.
- Label examples: "In Progress 3", "Todo 5", "Backlog 8", "Done 12".

### 3.6 Issue row (THE signature element)
Glass row (fill white 5%, stroke white 6% hairline, radius 10), content padding
**12px horizontal / 10px vertical**, total height ≈ 42px, **3px gap** between rows,
16px screen gutter. Single line, vertically centered. Left→right:

| # | Element | Spec |
|---|---------|------|
| 1 | Priority glyph | 16px column, semantic color (see §2), ~12pt glyph |
| 2 | gap 10px | |
| 3 | Identifier | monospace ~11–12pt, white 50%, **min-width 60px left-aligned** (fits "EXP-999"), e.g. `EXP-42` |
| 4 | gap 10px | |
| 5 | Status glyph | 16px column, semantic color |
| 6 | gap 10px | |
| 7 | **Title** | 15pt/14sp regular, white 100%, single line, ellipsizes — the ONLY flexible element |
| 8 | Label dots | up to 3 × 8px circles in label colors, 4px apart |
| 9 | gap 8px | |
| 10| Due date | calendar glyph 11–13px + "Today"/"Tomorrow"/"Jul 18" 12pt; color red/orange/white50 (§2); never wraps |
| 11| gap 8px | |
| 12| Assignee avatar | 22px circle, fill white 15%, initials white ~10pt medium (1 initial iOS, 2 Android) |
| 13| Android only | trailing `KeyboardArrowRight` chevron 16dp white 50% (iOS rows have NO chevron) |

Interactions (for animation): tap → detail push. iOS swipe-right-to-left reveals
green "Done" (`checkmark.circle.fill`) + gray "Cancel"; swipe left edge reveals orange
"Backlog". Android long-press opens a bottom action sheet ("Mark done" / "Move to backlog").
Empty list: centered "No issues yet" white 70%.

Optional repo strip (dev projects, iOS): full-width band under nav bar, fill white 4%,
containing a small repo chip (e.g. `Niach/exponential`).

---

## 4. Floating bottom navigation (both platforms)

A **left-aligned floating capsule pill** + a **detached circular compose button** on the
right, overlaid at the screen bottom (content scrolls beneath; lists reserve ~80–96px
clearance). Horizontal screen padding 20px, vertical 8px.

```
╭──────────────────────────────╮              ╭──────╮
│ [☰] [🔍] [🤖•] [📥•] [📦]   │   ←spacer→   │  ✎   │
╰──────────────────────────────╯              ╰──────╯
```

**The pill:** capsule; iOS `.ultraThinMaterial` (frosted) + stroke white 12% (0.5px)
+ shadow rgba(0,0,0,0.35) blur 16 y 6; Android near-opaque **`#151518` at 95% alpha**
(`0xF2151518`) + same stroke. Inner padding 5px.

**Five tabs** (exact order + icons):

| Tab      | iOS glyph (SF)      | Android glyph (Material) | Badge |
|----------|---------------------|--------------------------|-------|
| Issues   | `list.bullet`       | `List`                   | — |
| Search   | `magnifyingglass`   | `Search`                 | — |
| Agents   | custom robot-head asset (20px template) | `SmartToy` | 8px **green dot** when a coding session runs (iOS `#22C55E`, Android `#34D399`) |
| Inbox / My Work | `tray`       | `Inbox`                  | 8px dot when unread (iOS **indigo `#6366F1`**, Android primary `#E5E5E5`) |
| Releases | `shippingbox`       | `RocketLaunch`           | — |

- Tab hit area: 44×42 (iOS) / 48×42 (Android); icons ~17pt medium / 20dp.
- Active tab: capsule behind icon filled white 12%, icon white 100%; inactive icon white 70%.
- Badge dot sits top-right of the glyph area.
- **Compose button:** detached 52px circle, same material/stroke/shadow, icon
  `square.and.pencil` (iOS) / `Edit` pencil (Android) 20px white; a11y label "New issue".
  Hidden on surfaces without a project context (Search/Agents/Inbox).
- Bar shows only on top-level surfaces; detail/settings screens hide it.

---

## 5. Issue detail screen (secondary hero)

Same gradient background. iOS: back chevron + trailing share (`square.and.arrow.up`),
subscribe bell, and ⋯ overflow in the nav bar. Android: `CenterAlignedTopAppBar`
titled **"Issue"** with back arrow, `Share`, bell (`Notifications`/`NotificationsOff`,
tinted `#E5E5E5` when subscribed), `MoreVert` overflow.

Scrollable content, 20px padding, 20px section spacing:

```
[ EXP-42 ]  [ Niach/exponential ]            ← mono identifier chip (glass capsule,
                                               8h/4v padding, 12pt mono white50) + repo chip
Fix Electric sync loop on Android            ← editable title, 22pt semibold white
                                               (plain textfield, no box)
┌─ description markdown editor ─────────────┐
│ GFM blocks, checklists, images, @mentions │
└───────────────────────────────────────────┘
┌─ glassSection r=12, fill 4%, stroke 8% ───┐
│ Status      ⏳ In Progress                 │  ← rows: label 15pt white70 (80px col),
│ ───────────── divider white 6% ─────────── │    value right-aligned: glyph+label white
│ Priority    ⚠ Urgent                      │
│ ─────────────────────────────────────────  │
│ Assignee    Dennis Strähhuber              │  (hidden on solo workspaces)
│ ─────────────────────────────────────────  │
│ Release     📦 v1.0                        │  ("No release" white50 when empty)
└───────────────────────────────────────────┘
[ Due date inline calendar — glassSection ]
┌───────────────────────────────────────────┐
│ Repeat      Every 2 weeks / "Never"        │
└───────────────────────────────────────────┘
Labels                                       ← 15pt medium white70
( ● bug ) ( ● design ) ( + Label )           ← capsule glass pills, 8px dot + 12pt name;
                                               assigned = active fill 15%/stroke 20%
[ Changes card: PR diff / branch / "Being coded on <device>" ]
[ Attachments list ]
[ Comments thread + composer ]
```

Detail row: height ≈ 40px, label column 80px left-aligned, value right side with
6px icon-text gap. Each picker opens a bottom sheet (medium detent, frosted
`.ultraThinMaterial` background on iOS; M3 ModalBottomSheet on `#18181B` on Android).
Read-only viewers see the metadata card at 55% opacity. A "Coding now" steer section
and duplicate-of banner may appear above the metadata card.

---

## 6. Project switcher bottom sheet

Presented modal bottom sheet (iOS medium/large detents, frosted; Android
ModalBottomSheet, container `#18181B`, drag handle).

```
Switch project                                  ← 17pt semibold white (iOS title; Android omits)

  ⌂ Dennis's Workspace                     3    ← 18px rounded-square workspace monogram
                                                  (fill #E5E5E5 at 70%, dark initial) +
                                                  12pt semibold white85 name + mono count white50
  ┌──────────────────────────────────────────┐
  │ </> Exponential          EXP          ✓  │   ← glass row r=10, padding 16h/14v:
  └──────────────────────────────────────────┘     type glyph in project color, name 17pt white,
  ┌──────────────────────────────────────────┐     mono prefix white50, indigo ✓ on current
  │ ▦ Personal tasks          PER            │
  └──────────────────────────────────────────┘
  ┌──────────────────────────────────────────┐
  │ 📣 Feedback  🌐           FDB            │   ← feedback boards get a small globe
  └──────────────────────────────────────────┘
  [ + New project ]                              ← Android: full-width filled button (#E5E5E5)
```

Project-type glyphs: dev = `chevron.left.forwardslash.chevron.right` / code brackets;
tasks = `square.grid.2x2`; feedback = `megaphone`. Multi-account: hostname + email
header per server group; blocks 18px apart, project rows 6px apart.

---

## 7. "My Work" personal tab (Android) / Inbox (iOS)

Android merges Inbox + My Issues:
```
My Work                                   ← 32sp semibold white
(Inbox 3) (My Issues)      [Mark all read]   ← glass segment pills (14h/8v), unread count
                                               in primary #E5E5E5; TextButton right
[notification / issue rows...]
```
iOS keeps a separate Inbox tab (tray icon) and a "Assigned to you" My Issues surface.

---

## 8. Create issue sheet (compose)

Full-height sheet over frosted background. Title field: "Issue title" placeholder,
20pt medium, inside a rounded-10 box (fill white 4%, stroke white 8%), padding 16h/12v.
Below: markdown editor, then stacked metadata rows (Status/Priority/Assignee/Repeat)
each as `[glyph in semantic color] Label(white70) … value`, a labels picker, a
"Create more" toggle, and a Create button. Bottom sheet uses `.ultraThinMaterial` (iOS).

---

## 9. Platform differences cheat-sheet

| Aspect | iOS | Android |
|---|---|---|
| Font | SF Pro (system) | Roboto (system) |
| Glass | Real blur (`.ultraThinMaterial`) | Flat low-alpha white fills, no blur |
| Nav pill fill | Frosted material + white 12% stroke | `#151518` @ 95% + white 12% stroke |
| Tab icons | list.bullet / magnifyingglass / robot asset / tray / shippingbox | List / Search / SmartToy / Inbox / RocketLaunch |
| Inbox badge dot | Indigo `#6366F1` | `#E5E5E5` (M3 primary) |
| Agents dot | `#22C55E` | `#34D399` |
| Todo status color | Near-white `#FAFAFC` | Neutral `#A1A1AA` |
| "In Progress" casing | "In Progress" | "In progress" |
| Backlog/Todo glyphs | dashed ring vs plain ring | same ring for both |
| Large title | None (inline switcher in nav bar) | 32sp scrolling title above pills |
| Row trailing chevron | None | `KeyboardArrowRight` 16dp white50 |
| Row quick actions | Swipe (green Done / gray Cancel / orange Backlog) | Long-press bottom sheet |
| Avatar initials | 1 letter | up to 2 letters |
| 4th tab name | "Inbox" | "My Work" (Inbox + My Issues merged) |
| Switcher control | Bare text+chevron in nav principal slot | Glass capsule pill top-left |

## 10. Copy strings (exact)
"All issues" · "Active" · "Backlog" · "Todo" · "In Progress" · "Done" · "Cancelled" ·
"Duplicate" · "No priority" · "Urgent" · "High" · "Medium" · "Low" · "Today" ·
"Tomorrow" · "No issues yet" · "Syncing workspace…" · "Switch project" · "Clear all" /
"Clear" · "New issue" · "Issue" · "Status" · "Priority" · "Assignee" · "Unassigned" ·
"Release" · "No release" · "Repeat" · "Labels" · "+ Label" · "Mark done" ·
"Move to backlog" · "My Work" · "Inbox" · "My Issues" · "Mark all read" ·
"Create project" · "New project" · "No projects yet" · "Issue title" ·
"Create your first project to get started."

Identifiers look like `EXP-42` (project prefix + number). Realistic project names:
"Exponential" (prefix EXP, dev type), workspace "Dennis's Workspace" / "Feedback".
