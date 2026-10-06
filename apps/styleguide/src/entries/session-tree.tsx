import {
  AgentRunMark,
  conceptIcon,
  ListRow,
  type RunMarkState,
  TREE_BASE,
  TREE_INDENT,
  TreeGuides,
  treeGuides,
} from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-996: the session tree — a list of runs that nests a run under the run
// that started it and folds a resumed run into one row.
//
// EXP-1208: the specimen is the REAL row layout drawn with the real `@exp/ui`
// primitives (`ListRow`, `AgentRunMark`, `TreeGuides` over `treeGuides`),
// static rows instead of the four Electric collections `SessionTree` reads:
// [run mark][fold chevron, parents only][identifier + title], the mark at
// the row's base inset `12 + 14·depth` on EVERY row — so the parent's mark
// sits exactly under the standalone row's above it, and the child's elbow
// ends at the child's own mark. The sub-line aligns under the title.

const ChevronDownIcon = conceptIcon(`ui-chevron-down`)
const ChevronRightIcon = conceptIcon(`ui-chevron-right`)

interface SpecimenRun {
  depth: number
  /** The run mark's state; `ended` = a finished run (dimmed, no badge). */
  state: RunMarkState
  agent: string
  identifier: string | null
  title: string
  byline: string
  parent?: boolean
}

const RUNS: readonly SpecimenRun[] = [
  // A standalone run, then a parent run with the two it started.
  {
    depth: 0,
    state: `working`,
    agent: `claude`,
    identifier: `APP-88`,
    title: `Checkout rework`,
    byline: `Studio Mac · started 20m ago`,
  },
  {
    depth: 0,
    state: `review`,
    agent: `claude`,
    identifier: `APP-89`,
    title: `Plan the release train`,
    byline: `Ready for review · Studio Mac`,
    parent: true,
  },
  {
    depth: 1,
    state: `needs_input`,
    agent: `codex`,
    identifier: `APP-90`,
    title: `Refresh the store screenshots`,
    byline: `Needs input · Studio Mac`,
  },
  {
    depth: 1,
    state: `ended`,
    agent: `claude`,
    identifier: `APP-91`,
    title: `Write the changelog`,
    byline: `Studio Mac · 12m ago`,
  },
  // An unrelated chat run at top level.
  {
    depth: 0,
    state: `ended`,
    agent: `claude`,
    identifier: null,
    title: `Why is the board slow?`,
    byline: `Studio Mac · 2h ago`,
  },
]

/** The sub-line's tone — the session status line's palette. */
const STATUS_TONE: Record<RunMarkState, string> = {
  working: `text-muted-foreground`,
  needs_input: `text-amber-400`,
  review: `text-emerald-400`,
  done: `text-sky-400`,
  ended: `text-muted-foreground`,
}

function SessionTreeSpecimen() {
  const guides = treeGuides(RUNS.map((run) => run.depth))
  return (
    <div className="flex w-[420px] max-w-full flex-col">
      {RUNS.map((run, index) => (
        <ListRow
          key={run.title}
          interactive
          className="relative gap-2 px-3 py-2.5"
          style={{ paddingLeft: `${TREE_BASE + run.depth * TREE_INDENT}px` }}
        >
          <TreeGuides guide={guides[index]} />
          <AgentRunMark
            agent={run.agent}
            state={run.state}
            ringClassName="ring-background"
          />
          {run.parent && (
            <span className="flex w-3.5 shrink-0 items-center justify-center text-muted-foreground">
              <ChevronDownIcon className="size-3" />
            </span>
          )}
          <div className="min-w-0 flex-1">
            <div className="flex min-w-0 items-center gap-1.5 text-sm">
              {run.identifier && (
                <span className="shrink-0 font-mono text-xs text-muted-foreground">
                  {run.identifier}
                </span>
              )}
              <span className="truncate font-medium">{run.title}</span>
            </div>
            <div className={`truncate text-xs ${STATUS_TONE[run.state]}`}>
              {run.byline}
            </div>
          </div>
          {run.state === `ended` && (
            <ChevronRightIcon
              aria-hidden
              className="size-4 shrink-0 text-muted-foreground"
            />
          )}
        </ListRow>
      ))}
    </div>
  )
}

export const entry: StyleguideEntry = {
  id: `session-tree`,
  section: `special`,
  owner: `EXP-996`,
  title: `Session tree`,
  blurb: `Runs nested under their parent, resumes collapsed. ONE selector over the synced coding_sessions rows (\`sessionTree\`) that every sessions list draws: a resume succession is ONE row keyed by its newest, a \`sessions_start\` child nests under its parent's succession, an orphan whose parent is gone sits at top level. Top-level rows sort by last activity, newest first; children keep creation order; folding a parent takes its children with it. EXP-1208: every row ×4 LEADS with the run mark (the agent's brand mark, Claude's spark while it works, a state badge when parked, dimmed once ended; never a dot) at the row's base inset 12 + 14 per level, and a parent's fold chevron FOLLOWS the mark, so a parent lines up exactly with a standalone row and its child's connector elbow ends at the child's mark.`,
  status: {
    web: {
      state: `ok`,
      symbol: `SessionTree / sessionTree`,
      file: `apps/web/src/components/session-tree.tsx`,
      note: `the rule is lib/sessions/session-tree.ts; the rows are components/session-list-rows.tsx; drawn in every sessions list`,
    },
    desktop: {
      state: `ok`,
      symbol: `domain::session_tree::session_tree`,
      file: `apps/desktop/crates/domain/src/session_tree.rs`,
      note: `drawn by sidebar.rs (Running) and sessions_section.rs; the rows are run_rows.rs`,
    },
    ios: {
      state: `ok`,
      symbol: `SessionTree.sessionTree`,
      file: `apps/ios/ExpCore/Sources/Domain/SessionTree.swift`,
      note: `drawn by UI/Agent/AgentSessionsList.swift; the rows are UI/Session/RunningSessionRow.swift + ExpUI EndedRunRow`,
    },
    android: {
      state: `ok`,
      symbol: `SessionTree.sessionTree`,
      file: `apps/android/app/src/main/java/com/exponential/app/domain/SessionTree.kt`,
      note: `drawn by ui/agent/AgentSessionsList.kt; the rows are ui/session/RunningSessionRow.kt + ui/components/EndedRunRow.kt`,
    },
  },
  island: () => <SessionTreeSpecimen />,
}
