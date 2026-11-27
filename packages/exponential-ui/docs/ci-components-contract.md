# genUI: LogView, JobGraph and CodeBlock anchors (#EXP-2204)

The low-level pieces the CI run page, its log and the code page build on
(epic #EXP-2169, the approved CI and code redesign). This file is the
CONTRACT: what changed in `@exponential_app/eui` and what each of the five
renderers (React, Solid, gpui, Compose, SwiftUI) implements to match.
Everything here is locked by a fixture or a test.

## 0. What moved

| area | change | source of truth | locked by |
|---|---|---|---|
| status table | `ciStatus` enum passed\|failed\|running\|queued\|cancelled\|skipped; ONE table: the glyph + spin in `style.json ciStatus`, the colours in the theme recipes (`when: {status}`) | `core.catalog.json`, `style.json`, `themes/` | `ci-log.json` (`statusTable`), `theme-recipes.json` |
| line anchors | `L12` / `L6-L9`, one parser for LogView and CodeBlock | `src/ci-log.ts` `parseLineAnchor`, `formatLineAnchor`, `extendLineAnchor` | `ci-log.json` `anchors` |
| `LogView` | NEW macro (data, not lite): the `annotations` slot over the hidden native `LogLines` | `core.catalog.json`, `macros.json` | `catalog-macros.json` |
| `LogLines` | NEW hidden native: toolbar (search, n of m, prev/next, Follow, Timestamps) + the windowed log | `core.catalog.json`, `style.json logView`, `src/ci-log.ts` | `ci-log.json`, `catalog-macros.json` (through LogView) |
| `JobGraph` | NEW native (data, not lite): stages as columns, needs on hover/focus, matrix as one node, a status list below `md` | `core.catalog.json`, `style.json jobGraph`, `src/job-graph.ts` | `job-graph.json`, `catalog-components.json` |
| `CodeBlock` | `anchors`, `selection` (bindable), `gutter` numbers\|blame + `blame` data, event `select`, windowing, the pinned header | `core.catalog.json`, `style.json codeBlock`, `src/ci-log.ts` `blameBlocks` | `ci-log.json` `blame`, `catalog-components.json` |
| strings | `searchLog`, `matchCount`, `previousMatch`, `nextMatch`, `follow`, `timestamps`, `logView`, `jobGraph`, `lineAnchor`, `statusPassed`, `statusFailed`, `statusRunning`, `statusQueued`, `statusCancelled`, `statusSkipped` | `strings.json` | generated STRING tables |
| Swift test host | the FULL Lucide registry (not ~60 glyphs), so `ci-*` and every concept icon draw in snapshots | `packages/exponential-ui-swift` | Swift snapshots |

The pure rules (§2, §3, §5) are written ONCE per language: TypeScript
(`src/ci-log.ts`, `src/job-graph.ts`, the reference), Rust (`eui-core`
`ci_log.rs`, `job_graph.rs`: gpui), Swift (`ExponentialUIPrimitives`
`CiLog.swift`, `JobGraphLayout.swift`) and Kotlin (`primitives`
`CiLog.kt`, `JobGraphLayout.kt`), each replaying `fixtures/ci-log.json` and
`fixtures/job-graph.json`. Painters never re-derive them.

## 1. The status table (ONE on every platform)

| status | glyph (`style.json ciStatus`) | colour (recipe `when: {status}`) | text | motion |
|---|---|---|---|---|
| passed | `ci-passed` | `$color.success` | foreground | — |
| failed | `ci-failed` | `$color.destructive` | foreground | — |
| running | `ci-running` | `$color.info` (blue) | foreground | spins: one turn per `spinMs` (1000), linear; THE only moving mark; static under reduced motion |
| queued | `ci-queued` | `$color.mutedForeground` | foreground | — |
| cancelled | `ci-skipped` | `$color.mutedForeground` | `$color.mutedForeground` | — |
| skipped | `ci-skipped` | `$color.mutedForeground` | `$color.mutedForeground` | — |

No strikethrough anywhere. An unknown or missing status = `queued`. The
glyph is 16 px (`$control.iconSm`). Parts that take the colour: LogLines
`sectionIcon`, JobGraph `icon`; the text colour: LogLines `sectionTitle`,
JobGraph `name` (`when: {status: [cancelled, skipped]}`). Its a11y name is
`$string.status<Status>` ("Passed" …), read after the title.

## 2. Line anchors

`parseLineAnchor(text)`: an optional `#`, then `L<a>` or `L<a>-L<b>`
(a, b ≥ 1, decimal, no sign) → `{start: min(a,b), end: max(a,b)}`; anything
else → null. `formatLineAnchor(start, end)` = `L<start>` when equal, else
`L<start>-L<end>`. `extendLineAnchor(current, line, shift)`: no shift or no
current → `L<line>`; shift → the range from the current START to `line`
(normalised). Pressing a line number (LogLines gutter, CodeBlock with
`anchors`) sets the anchor this way, writes it back to a bound `selection`,
and fires `select {anchor, start, end}`. The selected lines take the `line`
part's `selected` state.

## 3. The log model (`src/ci-log.ts`)

`logModel(lines, sectionsMeta?)` → `{lines, sections}`:

- Each input string is one raw line. A leading timestamp
  `YYYY-MM-DDTHH:MM:SS(.fraction)?(Z|±HH:MM)` + ONE space is cut off into
  `time` = its `HH:MM:SS` characters as written (no zone conversion).
- Markers (after the timestamp, tested on the escape-free text):
  `##[group]Title` opens a section (title trimmed; a group while another is
  open closes that one first, no nesting); `##[endgroup]` closes the open
  one. Marker lines are NOT content: they get no number.
- `##[error]`, `##[warning]`, `##[notice]`, `##[debug]` prefixes are cut
  and colour the whole line `error`, `warning`, `comment`, `comment`
  (a codeToken kind, `kind` on the line).
- Content lines are numbered 1… in order (`n`); `section` = the index of
  the open section or -1. `text` = the line WITHOUT the timestamp and the
  prefix, ANSI escapes kept (painters tokenize it with the `ansi` language
  of `tokenizeCode`, lazily, per visible line, carrying no SGR state across
  lines).
- `sections[i]` = `{index, title, start, end, count, status, duration}`:
  `start`/`end` = the first/last content line numbers (`start` = `end + 1`
  and `count` 0 when empty), `status`/`duration` from `sectionsMeta[i]`
  (`{status, duration}`) when given. With no meta for it: the LAST section
  is `running` when its group never closed, else every section is `passed`.

`logOpenDefault(model)` = the sections whose status is `failed` or
`running` (the failed or running one opens on its own).

`logRows(model, open)` → the flat rows the painter windows: a content line
outside every section → `{kind: "line", n}`; each section → `{kind:
"section", section}` then, when its index is in `open`, its lines. Order =
the log order.

`logSearch(model, query)` → matches `[{line, start, end}]`: every
non-overlapping occurrence, left to right, of `query` in each content
line's ESCAPE-FREE text (folded sections included), ASCII case-insensitive
(A–Z = a–z, nothing else folds); offsets in Unicode code points; an empty
query → none.

`logReveal(model, open, line)` → `open` plus the section holding `line`
(sorted, unique): what opening a link or stepping to a match does.

`blameBlocks(blame, lineCount)` (CodeBlock) → per line `{block, first}`:
entries sorted by `start`, a block runs from its `start` to the next
entry's `start` − 1 (the last to `lineCount`); lines before the first entry
belong to no block (block −1).

## 4. LogView and LogLines

`LogView(lines~, sections~, open~, query~, selection~, follow~:b,
timestamps~:b, maxLines?:n, slots: {annotations}, on: {select, toggle,
search, follow, timestamps})`. The macro: root Box column, gap
`$spacing.md` → the `annotations` slot (the failure summary: an Alert, a
Card) → `LogLines` with every prop passed through (bindings stay bindings,
so its writes reach the author's data) and every event routed.

LogLines paints, top to bottom:

1. **toolbar** (row, height `$control.input` 36, gap `$spacing.sm`): the
   `search` field (an Input look, placeholder `$string.searchLog`, width
   `style.json logView.searchWidth` 240, shrinking), the `count` text
   `$string.matchCount` (`{current} of {total}`, muted, tabular; hidden for
   an empty query; `0 of 0` with none), `prev` / `next` icon buttons
   (`ui-chevron-up`/`ui-chevron-down`, `$string.previousMatch`/`nextMatch`,
   disabled without matches), a flexible spacer, then two toggle buttons
   `follow` (`$string.follow`) and `timestamps` (`$string.timestamps`),
   the Toggle outline look, `selected` state when on.
   **Compact** (ONE rule ×5, decided by the core): LogLines is `compact`
   when the SURFACE breakpoint is below `md`, the same signal as
   JobGraph's list mode (web `ciCompact`; Rust `!ctx.wide`; a breakpoint
   change rebuilds). The core passes it as the leaf prop `compact`. A
   compact toolbar is two rows, `$spacing.sm` apart: row 1 = `search`
   (fills the width) · `count` · `prev` · `next`; row 2 = `follow` ·
   `timestamps`, end-aligned. Wide = the one row above. No renderer wraps
   or squeezes on its own (no width heuristic).
2. **body** (`maxLines` × the line height tall, default 24 lines; a node
   style height or flex wins: it fills): the rows of `logRows`, WINDOWED
   (only rows intersecting the viewport ± `windowOverscan` are built),
   scrolling both ways (lines never wrap).
   - **section** row: `$control.row` (40) tall, the `chevron`
     (`ui-chevron-right`, turned down when open), `sectionIcon` (§1),
     `sectionTitle` (sm medium, truncated), `sectionMeta` (the duration,
     xs muted, tabular), pressable: toggles the section, fires `toggle
     {section, open}`, writes a bound `open`.
   - **line** row: the code line height (the `line` recipe's
     `lineHeight`: CodeBlock's default 22.75 with the neutral theme, mono
     `sm` type from the `body` recipe), `gutter` (the number, right-aligned, width = the
     widest number + 1 ch, PRESSABLE: §2), `time` (only with
     `timestamps`, muted), `code` (the ansi tokens; a `##[error]`-style
     `kind` colours the whole line), indented by one `logView.indent`
     (16) when inside a section. Search marks: every match a `match`
     part box (`$color.match`), the current one the same part in the
     `selected` state (`$color.matchCurrent`).
3. **state**: `open` = the bound/literal value, else `logOpenDefault`;
   `query`, `follow` (default TRUE), `timestamps` (default false) likewise,
   each written back when bound. Typing fires `search {query}`; the current
   match resets to the first; Enter = next, Shift+Enter = prev, both wrap;
   stepping to a match reveals it (`logReveal`) and scrolls it into view.
   A new `selection` (initial or changed from outside) reveals its start,
   scrolls the start line to the top third of the body and turns Follow
   OFF (written back, `follow {follow: false}`). Follow on: the body sticks
   to its bottom as lines arrive; a scroll up by the person (not by
   reveal) turns it off (`follow {follow: false}`); pressing the toggle
   back on jumps to the bottom.
4. **a11y**: root `region` named `$string.logView`; the body a polite
   live `list` (web: `role=log`, aria-live polite; `log` is no catalog
   a11y role, so a11y.json says `list`); section rows `button` with `expanded`; the gutter
   numbers `link` named `$string.lineAnchor` (`Line {line}`).

## 5. JobGraph (`src/job-graph.ts`)

`JobGraph(jobs~, value~, on: {select})`; `jobs` = `[{id, name, stage?,
status?, duration?, needs?: [id], matrix?}]`.

`jobGraphLayout(jobs, compact)` → `{mode, width, height, stages, nodes,
edges}`, geometry from `style.json jobGraph`:

1. **Matrix**: jobs sharing a non-empty `matrix` key collapse into ONE node
   at the first member's place: `id` = `name` = the key, `count` =
   `{passed, total}` (shown as `3/5` in place of the duration), status =
   the first present of failed, running, queued, cancelled, passed, skipped
   over the members; `needs` = the members' needs, mapped through the
   collapse, without itself, first-seen order.
2. **Columns**: when any node has a `stage`, columns = the stages in
   first-appearance order (a node without one takes `""`, also a column);
   else column = 0 for a node without known needs, else 1 + the max column
   of its needs (a need on the current path is ignored: cycles break).
   Within a column, input order.
3. **Graph geometry**: node `nodeWidth` × `nodeHeight` (200 × 40); x =
   column × (nodeWidth + `columnGap` 48); y = top + row × (nodeHeight +
   `rowGap` 12); top = `stageHeight` (24) + `stageGap` (8) when stages show,
   else 0. Width/height = the bounding box. `stages[i]` = `{title, x}` (only
   with stages). Edge per need: from (upstream right, upstream centre y) to
   (node left, node centre y), a cubic with both control points at the
   middle x (`c1 = (mx, y1)`, `c2 = (mx, y2)`).
4. **Upstream**: `jobGraphUpstream(layout, id)` = the node and every
   ancestor through `needs` (transitive). While a node is hovered or
   focused, ONLY the edges whose two ends are in that set draw (stroke
   `edgeWidth` 1.5, the `edge` part colour), and the set's nodes take the
   `highlight` state. No connector draws otherwise.
5. **compact** (the surface breakpoint below `md`): `mode: "list"`, every
   node a full-width row `$control.row` (40) tall in column order (column
   0 first, then input order), hairline dividers: `icon`, `name`, `meta`
   (duration or `3/5`), `chevron` (`ui-chevron-right`). Height = rows × 40.

Nodes: the `node` part (card fill, hairline border, radius md, padding
horizontal 12, gap 8): `icon` (§1) · `name` (sm medium, truncated) ·
spacer · `meta` (xs muted tabular). States: `hover`, `focus-visible`,
`highlight` (border `$color.ring`), `selected` (the node whose id = `value`:
`$color.muted` fill). Pressing a node (or a list row) sets a bound `value`
and fires `select {id}`. A graph wider than its box scrolls horizontally.
A11y: root `group` named `$string.jobGraph`; nodes `button` named `name,
status` (+ `3/5`), Tab order = column order.

## 6. CodeBlock

- `anchors: true`: the line numbers show (whatever `lineNumbers`) and are
  pressable (`lineNumber` part `hover` state, pointer cursor): §2.
- `selection` (bindable anchor): the lines in range take the `line`
  part's `selected` state (the `highlight` look); a block that scrolls
  itself (`maxLines`) scrolls the first selected line into view.
- `gutter: "blame"` + `blame` = `[{start, author, age}]`: a `blame` column
  (`style.json codeBlock.blameWidth` 200) BEFORE the numbers; the first
  line of each block (`blameBlocks`) shows `blameAuthor` (truncated) and
  `blameAge` (muted, end-aligned); every block after the first has a
  hairline top border (`blame` part `borderColor`) across the whole line.
  `gutter: "numbers"` (default) = today.
- Windowing: a block whose body scrolls (`maxLines`, or a node height) with
  more than `windowThreshold` lines builds only the lines in view ±
  `windowOverscan` (uniform line height: `wrap: false` only).
- The header (title + copy) never scrolls with the body: it stays pinned
  above it.
- Event `select {anchor, start, end}`.

## 7. Per-renderer checklist

Every renderer: (1) replay `ci-log.json` + `job-graph.json` (web: the TS
reference already does; gpui: `eui-core`; Swift/Kotlin: their primitives
port); (2) paint `LogLines`, `JobGraph`, the CodeBlock additions from the
parts above with the theme's recipes; (3) the windowing; (4) the events
and two-way writes; (5) a11y; (6) specimens render (`exponential-ui-log-view`,
`exponential-ui-job-graph`, `exponential-ui-code-block` new cases) and the
snapshot baselines are re-recorded.

## 8. Core decisions (what the TypeScript reference settled)

Every renderer and port follows these; the fixtures lock them.

**API** (`@exponential_app/eui`, both modules import-free):

```ts
CI_STATUSES: readonly ["passed","failed","running","queued","cancelled","skipped"]
CI_STATUS_SPIN_MS = 1000
CI_STATUS_TABLE: Record<CiStatus, {status, icon, spin, color, text, label}> // color/text = $color refs, label = string id
ciStatusOf(value: unknown): CiStatus                     // unknown/missing → "queued"
ciStatusEntry(value: unknown): CiStatusEntry
parseLineAnchor(text: unknown): {start, end} | null
formatLineAnchor(start: number, end = start): string    // order normalised
extendLineAnchor(current: unknown, line: number, shift: boolean): string
stripAnsi(text: string): string                         // complete CSI escapes removed (code.ts ansiRuns' rule)
logModel(lines: unknown[], sectionsMeta?: unknown[]): {lines: LogLine[], sections: LogSection[]}
  LogLine    = {n, section (-1 = none), time: "HH:MM:SS" | null, kind: "error"|"warning"|"comment"|null, text}
  LogSection = {index, title, start, end, count, status: CiStatus, duration: string | null}
logOpenDefault(model): number[]
logRows(model, open: unknown[]): ({kind: "line", n} | {kind: "section", section})[]
logSearch(model, query: unknown): {line, start, end}[]   // code points, end exclusive
logReveal(model, open: unknown[], line: number): number[]
blameBlocks(blame: unknown[], lineCount: number): {block, first, border}[]
JOB_GRAPH = style.json jobGraph (+ rowHeight 40)
jobGraphLayout(jobs: unknown[], compact: boolean, geometry = JOB_GRAPH):
  {mode: "graph"|"list", width, height, stages: {title, x}[], nodes: JobGraphNode[], edges: JobGraphEdge[]}
  JobGraphNode = {id, name, status, duration: string|null, count: {passed, total}|null, members: string[],
                  needs: string[], stage, column, row, x, y, width, height}
  JobGraphEdge = {from, to, start: [x, y], c1: [x, y], c2: [x, y], end: [x, y]}
jobGraphUpstream(layout, id): string[]
```

**Anchors.** Digits are ASCII, 1 to 9 of them (leading zeros allowed,
`L007` = 7); 0, a sign, a space, a lower-case `l`, `##` or any trailing
text = null. `extendLineAnchor` with shift and an unparsable current =
`L<line>`.

**Log model.** The timestamp needs its one trailing space (a line that IS
only a timestamp stays text). Markers and prefixes are tested at column 0
of the escape-free text (no leading whitespace, case-sensitive); a prefix
is cut by removing its characters from the text while every ANSI escape
around it stays. A group title is trimmed of ASCII spaces and tabs only; a
`##[group]` with nothing after it is a section titled `""`. `##[endgroup]`
with no open section is dropped (and anything after `##[endgroup]` on its
line is ignored). Meta: `sectionsMeta[i]` an object whose `status` is set
(not null) → `ciStatusOf(status)` (so a bogus one = queued); otherwise the
default rule; `duration` only when a string. `logRows`/`logReveal` take any
array: values that are no section index (or no integer, for reveal) are
ignored.

**Blame.** `block` = the index into the AUTHOR's `blame` array (so painters
read `blame[block]`), entries whose `start` is no integer ≥ 1 are ignored,
equal starts: the later entry wins (the earlier is empty). `border` = a
block's first line other than the first shown block's (the hairline).

**JobGraph.** Dropped jobs: no string `id`, a repeated id, a plain job
named like an earlier matrix key, a matrix member whose key names an
earlier plain job. A need may name a job id (mapped through the collapse)
or a matrix key itself; unknown needs and self needs are dropped, the rest
deduplicated in first-seen order. A matrix node takes its FIRST member's
stage; a single-member matrix still shows `1/1`. The depth columns are
computed depth-first in input order (needs in order, memoised; a need on
the current path is skipped). `nodes` come in COLUMN order (column, then
input order) in both modes; `edges` in that node order, needs in order;
`stages[i].x` = column × (nodeWidth + columnGap); no nodes = width/height 0.
List mode: `width` 0 (rows fill the box), every node `x` 0, `width` 0,
`height` = `rowHeight`, `y` = index × rowHeight, no stages or edges.

**Leaf sizes (the core).** A LogLines leaf without a node height or flex
is `toolbar + $spacing.sm + maxLines × lineHeight + 2 × ($spacing.sm +
$border.hairline)` tall, where `toolbar` = `$control.input` (wide) or `2 ×
$control.input + $spacing.sm` (compact): the body keeps `maxLines` lines in
both modes. Its resolved props add `compact` (bool). A JobGraph leaf is its
layout's height and carries `compact` likewise.

**Catalog.** LogView/LogLines `sections` is `[object]` (`{status,
duration}`), no named shape (the prompt budget). LogView's macro parts:
`root` (the region Box) and `log` (the LogLines node). The prompt keeps
its 10,000-token ceiling: descriptions across the catalog were shortened
and `fit`, `emptyText`, `href`, `cancel` joined the common props.

**Recipes** (`catalog/recipes.json`). States gain `highlight`;
`focus-visible` above = the recipe state `focus`.
- LogLines parts: `root`, `toolbar`, `search`, `count`, `prev`, `next`,
  `follow`, `timestamps`, `body`, `section`, `chevron`, `sectionIcon`,
  `sectionTitle`, `sectionMeta`, `line`, `gutter`, `time`, `code`,
  `token`, `match`; when-props `status` (section, sectionIcon,
  sectionTitle; always concrete), `kind` (line, token: a prefixed line
  paints its tokens with the LINE's kind; `plain` otherwise), `open`
  (section, chevron). States: the toggles `selected` when on, prev/next
  `disabled` without matches, `section` `hover`, `line` `selected` (in the
  anchor range), `gutter` `hover`, `match` `selected` (the current match),
  `search` `focus`.
- JobGraph parts: `root`, `stage`, `node`, `icon`, `name`, `meta`, `edge`
  (its `color` strokes the connector), `row`, `chevron`; when-prop
  `status` (node, row, icon, name). `node` states `hover`, `focus`,
  `highlight`, `selected`; `row` `hover`, `selected`.
- CodeBlock gains `blame` (its `borderColor` = the block hairline),
  `blameAuthor`, `blameAge`; `lineNumber` takes `hover` (anchors).

**Theme.** The neutral theme carries every look above (the
`exponential` theme adds only the muted LogLines `body` fill, as its
CodeBlock root). Status colours on `sectionIcon`/`icon`, the muted text
on `sectionTitle`/`name` for cancelled/skipped, no `textDecoration`.

**Strings.** `searchLog` "Search log", `matchCount` "{current} of
{total}", `previousMatch` "Previous match", `nextMatch` "Next match",
`follow` "Follow", `timestamps` "Timestamps", `logView` "Log", `jobGraph`
"Jobs", `lineAnchor` "Line {line}", `statusPassed` "Passed",
`statusFailed` "Failed", `statusRunning` "Running", `statusQueued`
"Queued", `statusCancelled` "Cancelled", `statusSkipped` "Skipped".

**Specimens** (`fixtures/specimens.json`): `exponential-ui-log-view` (a
failed job with timestamps, the `store` query and the failure Alert; a
running job with Follow), `exponential-ui-job-graph` (stages with the
`test` 3/5 matrix selected; needs only), `exponential-ui-code-block`
gains `anchors · selection L9-L11` and `gutter · blame` (src/train.ts,
three blocks). The kitchen sink does NOT hold LogView or JobGraph yet
(the web baseline and Compose goldens need re-recording first); the
coverage tests exempt them. No `views.json` entries yet.

**Web renderers** (React + Solid, one DOM; the painting helpers are
`@exponential_app/eui/web` `ci.ts`: `ciSegments` splits a line's ansi
tokens at its search marks, `ciWindow` = the core's `virtualWindow` over
fixed rows, `ciRevealOffset` = the top third, `ciCompact` = below `md`).
- LogLines rows are fixed: a step row = the `section` recipe's `height`
  (40 on every theme), a line = the `line` recipe's `lineHeight`; the body is `maxLines` × that tall plus its
  padding. A match across tokens wraps them in ONE `match` box; a
  prefixed line's tokens take the line's `kind`.
- The toolbar is two groups (find: search, count, prev, next; toggles:
  Follow, Timestamps, end-aligned). `ciCompact` (below `md`) sets
  `data-xui-compact` on the toolbar: the groups stack (`$spacing.sm`
  apart, each `$control.input` tall), the search fills the find row
  (React: LogLines is a breakpoint reader in `live.ts`, as JobGraph).
  Never a flex-wrap.
- A step row stays the body's width (sticky at the inline start) while
  long lines scroll sideways; every line row is as wide as the widest line
  (`ciColumns`), so the horizontal scroll never changes with the window.
- Typing a query reveals its first match; stepping reveals only a match
  out of view; a selection (from outside or the first one) always lands in
  the top third. A press on a line number turns Follow off too (it would
  scroll the pressed line away otherwise).
- JobGraph list rows flow (the `row` recipe's height, 40), the graph nodes are
  absolute from `jobGraphLayout`; a selected node is `aria-current`.
- CodeBlock's gutter (and LogLines') is content-box: `minWidth` = the
  widest number + 1 ch of TEXT, so numbers right-align past 9.
