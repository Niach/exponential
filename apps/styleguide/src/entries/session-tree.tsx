import { escapeHtml, svgChevronDown, svgChevronRight, svgLayers, svgWorkflow } from "../html.ts"
import type { StyleguideEntry } from "./types.ts"

// EXP-996: the session tree — what a list of runs looks like once it admits
// that a dozen rows are ONE thing.
//
// The specimen is deliberately hand-drawn markup rather than the app's own
// component: `SessionTree` reads four Electric collections and the router, and
// a specimen that needs a team to exist documents nothing. What the page has
// to show is the SHAPE — which rows are group rows, what each group's icon
// says, and how the connector nests a child run — and that is geometry.
//
// It draws in the page's own vocabulary (`.cmp-session-tree*` in
// `component-styles.ts`, the two glyphs in `html.ts`): EXP-1019 holds a filled
// entry to the same rules as a component demo, so no inline style and no class
// the stylesheet does not declare.

/** 14px of indent per level — `TREE_INDENT`, the ×4 constant, in the CSS. */
function row(depth: number, body: string, group = false): string {
  const cls = group ? `cmp-session-tree-row cmp-session-tree-group` : `cmp-session-tree-row`
  return `<div class="${cls}" data-depth="${depth}">${body}</div>`
}

/** A group row: the fold chevron, the group's icon, (a workflow's status
 *  dot,) its name, and its trailing cell — a workflow's `3 running · 5 of 8
 *  done` caption (EXP-1068), a stack's count. */
function group(
  depth: number,
  icon: string,
  name: string,
  trailing: number | string,
  { open = true, link = false, live = false }: { open?: boolean; link?: boolean; live?: boolean } = {}
): string {
  const label = link
    ? `<span class="cmp-session-tree-link">${escapeHtml(name)}</span>`
    : escapeHtml(name)
  return row(
    depth,
    [
      `<span class="cmp-session-tree-icon">${open ? svgChevronDown : svgChevronRight}</span>`,
      `<span class="cmp-session-tree-icon">${icon}</span>`,
      link ? `<span class="cmp-session-tree-dot"${live ? ` data-live` : ``}></span>` : ``,
      `<span class="cmp-session-tree-name">${label}</span>`,
      `<span class="cmp-session-tree-count">${escapeHtml(String(trailing))}</span>`,
    ].join(``),
    true
  )
}

/** A run row: its state dot, the identifier, the title. EXP-1068: a review
 *  under its node has no identifier, and a doubled node wears the warning. */
function run(
  depth: number,
  identifier: string | null,
  title: string,
  { live = true, warning = false }: { live?: boolean; warning?: boolean } = {}
): string {
  return row(
    depth,
    [
      `<span class="cmp-session-tree-dot"${live ? ` data-live` : ``}></span>`,
      identifier ? `<span class="cmp-session-tree-id">${escapeHtml(identifier)}</span>` : ``,
      `<span class="cmp-session-tree-title">${escapeHtml(title)}</span>`,
      warning ? `<span class="cmp-session-tree-count" title="Two live runs on this node">⚠</span>` : ``,
    ].join(``)
  )
}

export const entry: StyleguideEntry = {
  id: `session-tree`,
  section: `special`,
  owner: `EXP-996`,
  title: `Session tree`,
  blurb: `Runs nested under their parent, workflow and stack groups, resumes collapsed. ONE selector over the synced coding_sessions rows (\`sessionTree\`) that every sessions list draws: a resume succession is ONE row keyed by its newest, a \`sessions_start\` child nests under its parent's succession, and a stack sits under its own group row in linear order, lowest first. EXP-1068: a workflow's runs group by the row's server-stamped \`workflow_id\` (no heuristics), under a group row that wears the workflow's NAME, its status dot and \`3 running · 5 of 8 done\`; inside it one row per node's author run, the node's review runs nested under it as \`Review r2 · approved\`, base merges and plan runs as plain children; a node with two live runs wears the warning glyph, an open question the red dot. Stacks and workflows are not unified, the icon is the whole difference: nav-workflows for the graph, pr-stack for the chain. Groups and top-level rows sort by last activity, newest first; children keep creation order; a group is its children, so folding one takes them with it and a childless group never draws.`,
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
      `<div class="cmp-session-tree">`,
      // A workflow: its name is the only thing that links onward; the
      // trailing cell says how it stands. A review nests under its node.
      group(0, svgWorkflow, `EXP-996 +5`, `3 running · 1 of 3 done`, { link: true, live: true }),
      run(1, `EXP-1048`, `Session tree (web)`),
      run(2, null, `Review r2 · changes requested`),
      run(1, `EXP-1049`, `Session tree (IDE)`, { warning: true }),
      run(1, `EXP-1050`, `Session tree (iOS + Android)`, { live: false }),
      // A stack is a linear group: lowest first, and nowhere to go.
      group(0, svgLayers, `Stacked pull requests`, 2),
      run(1, `APP-41`, `Extract the merge queue`),
      run(1, `APP-42`, `Retry a failed merge`),
      // A parent run and its `sessions_start` child, one level deeper.
      run(0, `APP-88`, `Plan the release train`),
      run(1, `APP-89`, `Bump the iOS build number`),
      // Folded: the group stays, its runs are gone.
      group(0, svgWorkflow, `REV2-12 +2`, `3 of 3 done`, { open: false, link: true }),
      `</div>`,
    ].join(``),
}
