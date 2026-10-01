import { escapeHtml } from "../html.ts"
import type { StyleguideEntry } from "./types.ts"

// EXP-996: the session tree — a list of runs that nests a run under the run
// that started it and folds a resumed run into one row.
//
// The specimen is deliberately hand-drawn markup rather than the app's own
// component: `SessionTree` reads four Electric collections and the router, and
// a specimen that needs a team to exist documents nothing. What the page has
// to show is the SHAPE — how the connector nests a child run — and that is
// geometry.
//
// It draws in the page's own vocabulary (`.cmp-session-tree*` in
// `component-styles.ts`): EXP-1019 holds a filled
// entry to the same rules as a component demo, so no inline style and no class
// the stylesheet does not declare.

/** 14px of indent per level — `TREE_INDENT`, the ×4 constant, in the CSS. */
function row(depth: number, body: string): string {
  return `<div class="cmp-session-tree-row" data-depth="${depth}">${body}</div>`
}

/** A run row: its state dot, the identifier (none on an action or chat run),
 *  the title. */
function run(
  depth: number,
  identifier: string | null,
  title: string,
  { live = true }: { live?: boolean } = {}
): string {
  return row(
    depth,
    [
      `<span class="cmp-session-tree-dot"${live ? ` data-live` : ``}></span>`,
      identifier ? `<span class="cmp-session-tree-id">${escapeHtml(identifier)}</span>` : ``,
      `<span class="cmp-session-tree-title">${escapeHtml(title)}</span>`,
    ].join(``)
  )
}

export const entry: StyleguideEntry = {
  id: `session-tree`,
  section: `special`,
  owner: `EXP-996`,
  title: `Session tree`,
  blurb: `Runs nested under their parent, resumes collapsed. ONE selector over the synced coding_sessions rows (\`sessionTree\`) that every sessions list draws: a resume succession is ONE row keyed by its newest, a \`sessions_start\` child nests under its parent's succession, an orphan whose parent is gone sits at top level. Top-level rows sort by last activity, newest first; children keep creation order; folding a parent takes its children with it. A run parked on an open question wears the red needs-you dot.`,
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
      // A parent run, its `sessions_start` children one level deeper, and a
      // grandchild under the first of them.
      run(0, `APP-88`, `Plan the release train`),
      run(1, `APP-89`, `Bump the iOS build number`),
      run(2, `APP-90`, `Refresh the store screenshots`),
      run(1, `APP-91`, `Write the changelog`, { live: false }),
      // An unrelated chat run at top level.
      run(0, null, `Why is the board slow?`, { live: false }),
      `</div>`,
    ].join(``),
}
