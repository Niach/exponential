# Shell parity: the Exponential IDE → eDE (#EDE-46)

Every region of the IDE's shell (`apps/desktop/crates/ui/src`) against the
eDE region that replaces it, the genUI components it is built from (all in
`packages/exponential-ui/catalog/core.catalog.json`), the IDE's numbers as
read from the code (rem = 14 px: `theme/src/lib.rs` sets `font_size = px(14)`,
so `px_2` = 7 px and `gap_1` = 3.5 px) against eDE's numbers
(`fixtures/shell-layout.json`), and a status:

- `same`: one look.
- `close`: a known small difference, named in the note.
- `gap`: something genUI or eDE does not have yet, named in the note.

Masterplan decisions (session d6b83b10, approved 2026-11-06) this table
reflects: eDE has no top panel and no top tab band; its taskbar is Mint's
grouped window list at the bottom (icon-only, pinning, grouping, hover
previews) with the applets and the clock at the right end; the sidebar nav is
Needs you + Chats (no Assistant entry); bottom-left = the Exponential start
button and a chat button; Settings = the nav in the sidebar's place and the page on the ground's panel (#EDE-65; before, one window with a sidebar nav: This
device, Personal, Team); Files launches Nemo; Version history is a start-menu
app entry. The IDE's sidebar never folds (EXP-723). #EDE-65 (Danny's
decisions, superseding rows 3 to 9 where they differ): eDE's sidebar is the
IDE's 56 px icon rail for good, black like the IDE's (no Sidebar fill), and
lists only the chats (`chats` in `shell.json`), each the agent's mark with its
dot; no switcher, inbox, search, nav or sections there; the taskbar spans the
bottom edge and starts with the start, chat and new-issue buttons.

Colour tokens both sides read from `packages/design-tokens` (`glass`):
`FILL_PANEL` white 10/255, `FILL_CARD` 15, `FILL_ACTIVE` 28, `FILL_ROW` 15,
`STROKE_CARD` white 26/255, `STROKE_STRONG` 31, `STROKE_ACTIVE` 41.

## The IDE's regions

| # | IDE region (file) | eDE region | genUI components | IDE numbers | eDE numbers | status | note |
|---|---|---|---|---|---|---|---|
| 1 | Ground gradient (`shell.rs` `background_gradient`) | the wallpaper behind every window; the sidebar and the taskbar are panels over it | Box (`$color.background`) | 180° from `BACKGROUND_TOP` mixed 60 % to `BACKGROUND_BOTTOM` (≈ rgb 12,12,15) to rgb 17,17,20; alpha 0.96 with a blurred backdrop, else 1 | the compositor's Tier A ground from the same tokens, alpha 1, a wallpaper when set | close | a desktop shows a wallpaper, not a gradient; the panels keep `FILL_PANEL` over it |
| 2 | Rounded content card (`shell.rs` `PANEL_MARGIN`, `surface.rs` `PANEL_BORDER`) | no card on the desktop (apps are windows with Tier A frames); its numbers frame every eDE panel (start menu, previews, button menu, Settings window) | Box (`radius`, `border`), Popover | radius 12 (`radius::LG`), margin 10 (top 0 under the band, bottom 6 with the session bar), border 1 `STROKE_CARD`, fill `FILL_PANEL`, inner radius 11 | `panelRadius` 12, `panelMargin` 10, border 1 `STROKE_CARD`, fill `FILL_PANEL` | same | by decision no content card; every panel carries the card's frame |
| 3 | Sidebar column (`sidebar.rs`, `resize_edge::panel_width`) | the sidebar document, a left layer surface | Sidebar (`width`, `rail`) | 272 (drag 272…560, 8 px handle), never folds | `railWidth` 56 for good (#EDE-65), no drag | close | eDE's sidebar is the IDE's icon rail, never the 272 column |
| 4 | Header: team switcher (`render_left_column_header`) | the desktop/team switcher (`switcher.current`, `switcher.entries`) | Button (ghost), Avatar, Text, Icon, Menu (`hostedPopups`) | ghost button h 40, avatar 28, name `text_sm` semibold, xsmall chevron, live dot at -2/-2; menu = teams + New team + Join team | same button and avatar; menu = Local + the teams (`switcher.set`) | close | the local desktop is the first entry; New team and Join team live in the Exponential app |
| 5 | Header: inbox (`InboxButton`) | the inbox button | Button (icon), Badge (dot) | `web_icon_sm` 32, unread dot top 4 / right 4 | 32, same dot | same | |
| 6 | Header: search | the search button | Button (icon, ghost) | 32 circle | 32 | same | |
| 7 | Header: new issue | the new button | Button (icon, primary) | 32, only with a board | 32, only with a board | same | |
| 8 | Nav: Agent (`ACTION_CHAT`, the Home screen) | none; the chat button bottom-left (`chat.toggle`) opens the assistant | Button (icon) | row 28 | `taskbarButtonSize` 40 next to the start button | close | dropped from the nav by decision; the chat button replaces it |
| 9 | Nav: Needs you (`NAV_NEEDS_YOU`) | Needs you | Row (`icon`, `title`, `meta`), Badge (count) | `flat_row_compact` 28, `px_2` 7, `gap_2` 7, `text_sm`, radius 10, count badge; hidden at 0 | `sidebarRowHeight` 28, same badge, hidden at 0 | same | |
| 10 | Nav: Chats (`NAV_CHATS`, `chat-list.json` copy.title) | Chats | Row, Badge (dot amber/green/primary) | 28, status or unread dot | 28, same dots | same | |
| 11 | Nav: Devices (`NAV_DEVICES`) | none; the devices live in the Exponential app window | Row | 28 | — | close | dropped from the nav by decision (nav = Needs you + Chats) |
| 12 | Nav: Drafts (`NAV_DRAFTS`) | none; drafts live in the Exponential app window | Row, Badge | 28, hidden at 0 | — | close | dropped from the nav by decision |
| 13 | Section: Pinned (`rail_section_label`, unpin on hover) | Pinned (`sidebar.pinnedOpen`) | Section (`collapsible`, `title`), Row (`icon`, `title`), Button (unpin, hover) | label 24 (`text_xs` medium, foreground 0.5, `pl_1p5` `pr_1`), rows 28, section gap 8, never folds | `sectionLabelHeight` 24, rows 28, gap 8, folds | close | eDE folds the section |
| 14 | Section: Agents (+ New agent) | none in the sidebar; the Exponential app window | Section, Row, Button (`+`) | 24 + 28 | — | close | by decision the desktop sidebar holds Pinned, Workspaces, Sessions; agents are the app's |
| 15 | Section: Apps (+ New app / Import) | none in the sidebar; the start menu's Exponential group (`startMenuApps`, category `exponential`) | Section, Row, Menu | 24 + 28 | start-menu rows | close | by decision the apps are start-menu entries |
| 16 | Section: Boards (+ New board, settings gear on hover) | none in the sidebar; the Exponential app window | Section, Row, Button | 24 + 28 | — | close | by decision |
| 17 | Section: Active (`chat-list.json` copy.rail, hidden while empty) | Sessions (`sidebar.sessionsOpen`) | Section (`collapsible`), Row (`icon`, `title`, `subtitle`) | label 24, rows 28, row gap 3.5 (`RUNNING_ROW_GAP` = 0.25 rem), hidden while empty | 24, 28, row gap 4, hidden while empty, folds | close | the row gap rounds 3.5 → 4; eDE folds |
| 18 | What's new card (`render_whats_new_card`) | the same card at the sidebar's bottom | Row (`surface` card, `icon`, `title`), Button (close) | rounded 12, `STROKE_CARD`, `FILL_CARD`, `p_3` 10.5, `FILL_ACTIVE` hover, 80 px clearance | radius 12, same tokens, padding 10, clearance 80 | same | `p_3` is 10.5 at rem 14, drawn as 10 |
| 19 | Footer: account row (small avatar + first name; menu What's new, About, Sign out) | the user row | Button (ghost), Avatar (small), Text, Menu (`hostedPopups`) | h 36, `px_1p5` 5, menu opens upward | 36, 5, upward | same | |
| 20 | Footer: computer menu (Terminal, Files, Source Control + badge) | the computer menu: Terminal launches the terminal, Files launches Nemo | Button (ghost, icon), Menu, Badge | ghost small, opens upward | same | close | Source Control is not a desktop item; Files = Nemo by decision |
| 21 | Footer: settings gear (`NAV_SETTINGS`) | opens the Settings window | Button (ghost, icon) | ghost small | same | same | |
| 22 | Footer: voice button (`SETTINGS_VOICE`, optional) | the sound applet at the taskbar's right end | Button (icon), Popover, Slider (`icon`, `showValue`) | ghost small | `appletButtonSize` 32 | close | moved to the applets by decision |
| 23 | Footer: getting-started row + sync spinner | the same rows | Row, Spinner, Text | row 28 | 28 | same | |
| 24 | Left column top strip (34 px, traffic lights) | none: a layer surface has no client chrome | — | 34 | 0 | close | by decision: no window chrome on a desktop |
| 25 | Work-tabs band (`app_title_bar.rs` `WORK_TABS_BAND_H`) | none: the taskbar at the bottom is Mint's grouped window list (`taskbarButtons`) | Toolbar, Button (icon), Badge (running dot) | band 44, `BAR_INSET` 8, min strip width 160 | `taskbarHeight` 48, `taskbarButtonSize` 40, icon-only | close | by decision: no top band; a taskbar instead |
| 26 | Tab chip (`surface.rs` `strip_chip_frame`) | a taskbar button (icon, running dot, grouped count) | Button (icon), Badge (dot) | `STRIP_CHIP_H` 32, `px` 10, gap 4, radius 6, border 1 (`STROKE_STRONG` + `FILL_ACTIVE` active), max 240, title max 180 | 40 × 40, no title, active = `FILL_ACTIVE` | close | icon-only by decision (Mint's list) |
| 27 | Tab contents (`rich_tab`: 14 px lead glyph, mono id, title, exit badge, undock, close `web_icon_xs` 24; middle-click close; Close / Close others / Close all menu) | the button's menu (`taskbar.menu`: Pin/Unpin, Close, Close all) and the hover previews (`taskbar.previews`) | Menu (`hostedPopups`), Popover, Image (thumbnail), Text | 24 px close, context menu | previews tile 184 × 110, title under it, close on hover; menu items 28 | gap | the host's `/windows` carries `{id, title, focused, workspace}` only: `app` and `minimized` (and a per-window thumbnail outside the switcher) are missing for grouping and previews |
| 28 | Session bar (`session_bar.rs` `SESSION_BAR_H`: terminal tabs + `+`) | none: terminals are windows in the taskbar's terminal group | — | 36, `px_2`, 0 without a terminal tab | — | close | by decision: no session bar |
| 29 | Settings nav (`settings/mod.rs` `NAV_GROUPS`, `settings-nav.json`: Team, Boards, Agents, Apps, This device, Personal; back row 40; rows `px_2 py_1 gap_2 text_sm`; group label `text_xs` semibold; only Issues folds) | the Settings window's sidebar nav (`settings.json`, #EDE-47): This device (the Coding group folds), Personal, Team headed by the switcher | Sidebar (`width`), Row (`icon`, `title`, `depth`), Popover | panel 272, back row 40, row = padding + line (≈ 28), labels `text_xs` | 272, header row 40 (Settings + close), rows 28, labels 24, the groups fold like the sidebar's sections | close | three groups by decision; a team entry shows the IDE's Team, Boards, Agents and Apps groups (none yet, #EDE-54) |
| 30 | Settings page rows (`SETTINGS_ROW_H`) | the same rows | SettingsRow (`label`, `description`), Switch, Select, Input | 40 | 40 | same | |
| 31 | Settings controls: dates, levels, usage, trees (the IDE's rows under Personal and This device) | Date & time (inline calendar), Sound (level sliders), Storage (usage meter), Files/Worktrees (tree) | DatePicker (`inline`), Slider (`icon`, `endLabel`, `showValue`), Meter (`segments`), Tree | row 40 | 40 | same | |
| 32 | Brand glyph (the IDE's About row and the sidebar header avatar fallback) | the start button bottom-left | Icon (`brand-exponential`), Button | — | `taskbarButtonSize` 40 | same | the icon exists in `packages/icons` |

Status counts: 32 rows, `same` 13, `close` 18, `gap` 1 → 31 of 32 rows (97 %)
are `same` or `close`.

## The IDE's move (#EXP-1722)

The IDE mounts the same documents in place (`uicore_gpui::inline`), region by
region (#EXP-1725); each step ships with a headless picture, the feature
checklist and frame times (`crates/ui/src/shell_bench.rs`), then the gpui code
goes. The rows of the left column are `sidebar.json`, fed from `/shell/ide`
(`fixtures/ide-sidebar.json`); a row's glyph (a tinted board, the agent's
animated mark, a dot, a count) is painted by the host (`HostPlugin::icon_element`,
named `ide:<kind>:<id>`), so the document stays the catalog's.

The bottom tab bar is `docs/tabbar.json`, the IDE's own document (eDE keeps
Mint's icon-only `taskbar.json`). The IDE's Local desktops are `elocal`
(`crates/ui/src/local_desktops.rs`): the same store, executor and runs as eDE.

## eDE-only regions (not counted by the gate)

| eDE region | state / selector | genUI components | numbers |
|---|---|---|---|
| Start button + chat button (bottom-left) | `startMenu.toggle`, `chat.toggle` | Button (icon `brand-exponential`), Button (icon) | 40 each, in the 48 taskbar |
| Taskbar: grouped window list | `taskbarButtons`, `taskbarPress`, `taskbarScroll`, `taskbar.hover` | Toolbar, Button, Badge (running dot) | button 40 |
| Taskbar: hover previews | `taskbar.previews` | Popover (`hostedPopups`), Image, Text | tile 184 × 110 |
| Taskbar: applets + clock (right end) | the host's data (`/time`, tray, notifications) | Button (icon) ×n, Text, Popover | applet 32 |
| Workspace badges | `workspaceBadges`, `workspace.switched` | Segmented (rail), Badge (count) | row 28 |
| Sidebar section Workspaces | `sidebar.workspacesOpen` | Section (`collapsible`), Row, Badge | label 24, rows 28 |
| Alt-Tab switcher | `altTab.*`, `altTabOrder` | Box, Image, Text (`switcher.json` today) | overlay sized to its tiles (208 px each), 204 high |
| Start menu (Mint's three columns: favourites + places + power, categories, apps; search at the bottom) | `startMenuApps`, `startMenuCategories`, `startMenu.query`, `startMenu.category` | Grid, List, Row (`icon`, `title`, `subtitle`), Input, Button, Separator | 760 × 620 |
| Settings window (#EDE-47) | `settings.json`: the nav This device / Personal / Team beside the page | Sidebar, Section, Row, SettingsRow, Switch, Select, Slider, DatePicker (`inline`), EmptyState, Popover | over the rail and the ground, above the taskbar (#EDE-65): nav 272 (`settingsWidth`) where the rail is, the page on the ground's panel, back row 40, rows 28, labels 24 |

## Known gaps to close before the gate

1. Row 27: `ede-shell` must feed `app` and `minimized` on `/windows` (and
   a thumbnail per window for the previews), or `taskbarButtons` cannot
   group and `taskbarPress` cannot minimize.

## The gate rule

The parity gate (#EDE-45) passes when ≥ 90 % of the rows of the first table
are `same` or `close` AND Danny signs off by eye on the goldens
(`shots/ede-*`, captured by `bun run shots -- --platform ede --skip-seed`).
