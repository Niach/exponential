import { escapeHtml } from "../html.ts"
import type { StyleguideEntry } from "./types.ts"

// EXP-996: the session tree — what a list of runs looks like once it admits
// that a dozen rows are ONE thing.
//
// The specimen is deliberately hand-drawn markup rather than the app's own
// component: `SessionTree` reads four Electric collections and the router, and
// a specimen that needs a team to exist documents nothing. What the page has
// to show is the SHAPE — which rows are group rows, what each group's icon
// says, and how the connector nests a child run — and that is geometry.

/** Lucide's stroke geometry, verbatim (`html.ts` keeps its own copy of this
 *  wrapper private; two glyphs are not worth widening its surface). */
function glyph(body: string): string {
  return [
    `<svg class="glyph" viewBox="0 0 24 24" fill="none" stroke="currentColor"`,
    ` stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"`,
    ` style="width:14px;height:14px;flex:none">`,
    body,
    `</svg>`,
  ].join(``)
}

// The two group icons, named by CONCEPT: `nav-workflows` (lucide `workflow`)
// and `pr-stack` (lucide `layers`), exactly what the four clients draw.
const WORKFLOW = glyph(
  `<rect width="8" height="8" x="3" y="3" rx="2"/><path d="M7 11v4a2 2 0 0 0 2 2h4"/><rect width="8" height="8" x="13" y="13" rx="2"/>`
)
const STACK = glyph(
  [
    `<path d="M12.83 2.18a2 2 0 0 0-1.66 0L2.6 6.08a1 1 0 0 0 0 1.83l8.58 3.91a2 2 0 0 0 1.66 0l8.58-3.9a1 1 0 0 0 0-1.83z"/>`,
    `<path d="M2 12a1 1 0 0 0 .58.91l8.6 3.91a2 2 0 0 0 1.65 0l8.58-3.9A1 1 0 0 0 22 12"/>`,
    `<path d="M2 17a1 1 0 0 0 .58.91l8.6 3.91a2 2 0 0 0 1.65 0l8.58-3.9A1 1 0 0 0 22 17"/>`,
  ].join(``)
)
const CHEVRON_DOWN = glyph(`<path d="m6 9 6 6 6-6"/>`)
const CHEVRON_RIGHT = glyph(`<path d="m9 6 6 6-6 6"/>`)

/** 14px of indent per level — `TREE_INDENT`, the ×4 constant. */
const INDENT = 14
const BASE = 8

const ROW = [
  `display:flex`,
  `align-items:center`,
  `gap:6px`,
  `height:32px`,
  `padding-right:8px`,
  `font-size:13px`,
  `border-radius:var(--r-md)`,
].join(`;`)

function row(depth: number, body: string, extra = ``): string {
  return `<div style="${ROW};padding-left:${BASE + depth * INDENT}px;${extra}">${body}</div>`
}

const MONO = `font-family:ui-monospace,monospace;font-size:11px;color:var(--muted-fg);flex:none`
const COUNT = `${MONO};margin-left:auto`

/** A group row: the fold chevron, the group's icon, its name, its count. */
function group(
  depth: number,
  icon: string,
  name: string,
  count: number,
  { open = true, link = false }: { open?: boolean; link?: boolean } = {}
): string {
  const label = link
    ? `<span style="text-decoration:underline;text-underline-offset:2px">${escapeHtml(name)}</span>`
    : escapeHtml(name)
  return row(
    depth,
    [
      `<span style="color:var(--muted-fg);display:flex">${open ? CHEVRON_DOWN : CHEVRON_RIGHT}</span>`,
      `<span style="color:var(--muted-fg);display:flex">${icon}</span>`,
      `<span style="font-weight:500;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">${label}</span>`,
      `<span style="${COUNT}">${count}</span>`,
    ].join(``),
    `background:var(--row)`
  )
}

/** A run row: its state dot, the identifier, the title. */
function run(depth: number, identifier: string, title: string, tone: string): string {
  return row(
    depth,
    [
      `<span style="width:6px;height:6px;flex:none;border-radius:50%;background:${tone}"></span>`,
      `<span style="${MONO}">${escapeHtml(identifier)}</span>`,
      `<span style="min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">${escapeHtml(title)}</span>`,
    ].join(``)
  )
}

export const entry: StyleguideEntry = {
  id: `session-tree`,
  section: `special`,
  owner: `EXP-996`,
  title: `Session tree`,
  blurb: `Runs nested under their parent, workflow and stack groups, resumes collapsed. ONE selector over the synced coding_sessions rows (\`sessionTree\`) that every sessions list draws: a resume succession is ONE row keyed by its newest, a \`sessions_start\` child nests under its parent's succession, a workflow's node runs sit under one group row whose name LINKS to the workflow, and a stack sits under its own group row in linear order, lowest first. Stacks and workflows are not unified — the icon is the whole difference: nav-workflows for the graph, pr-stack for the chain. Groups and top-level rows sort by last activity, newest first; children keep creation order; a group is its children, so folding one takes them with it and a childless group never draws. A group row is not a run: no state dot, no device, no kill.`,
  status: {
    web: {
      state: `ok`,
      symbol: `SessionTree / sessionTree`,
      file: `apps/web/src/components/session-tree.tsx`,
      note: `the rule is lib/sessions/session-tree.ts; drawn in the sidebar's Running section and every sessions list`,
    },
    desktop: {
      state: `ok`,
      symbol: `domain::session_tree::session_tree`,
      file: `apps/desktop/crates/domain/src/session_tree.rs`,
      note: `drawn by sidebar.rs (Running) and sessions_section.rs`,
    },
    ios: {
      state: `ok`,
      symbol: `SessionTree.sessionTree`,
      file: `apps/ios/ExpCore/Sources/Domain/SessionTree.swift`,
      note: `drawn by UI/Agent/AgentSessionsList.swift`,
    },
    android: {
      state: `ok`,
      symbol: `SessionTree.sessionTree`,
      file: `apps/android/app/src/main/java/com/exponential/app/domain/SessionTree.kt`,
      note: `drawn by ui/agent/AgentSessionsList.kt`,
    },
  },
  render: () =>
    [
      `<div style="display:flex;flex-direction:column;width:420px;max-width:100%">`,
      // A workflow: its name is the only thing that links onward.
      group(0, WORKFLOW, `EXP-996 +5`, 3, { link: true }),
      run(1, `EXP-1048`, `Session tree (web)`, `var(--ok)`),
      run(1, `EXP-1049`, `Session tree (IDE)`, `var(--ok)`),
      run(1, `EXP-1050`, `Session tree (iOS + Android)`, `var(--muted-fg)`),
      // A stack is a linear group: lowest first, and nowhere to go.
      group(0, STACK, `Stacked pull requests`, 2),
      run(1, `APP-41`, `Extract the merge queue`, `var(--ok)`),
      run(1, `APP-42`, `Retry a failed merge`, `var(--ok)`),
      // A parent run and its `sessions_start` child, one level deeper.
      run(0, `APP-88`, `Plan the release train`, `var(--ok)`),
      run(1, `APP-89`, `Bump the iOS build number`, `var(--ok)`),
      // Folded: the group stays, its runs are gone.
      group(0, WORKFLOW, `REV2-12 +2`, 3, { open: false, link: true }),
      `</div>`,
    ].join(``),
}
