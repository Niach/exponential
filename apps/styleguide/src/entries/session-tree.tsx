import {
  conceptIcon,
  ListRow,
  LiveDot,
  TREE_BASE,
  TREE_INDENT,
  TreeGuides,
  treeGuides,
} from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-996 — the session tree: what a list of runs looks like once it admits
// that a dozen rows are ONE thing.
//
// EXP-1030 turned the hand-drawn specimen into a real island. The app's own
// `SessionTree` reads four Electric collections and the router, so it cannot
// be mounted here — but everything the page has to SHOW is chrome the package
// already owns: `ListRow` is the row, `treeGuides` + `TreeGuides` are the
// EXP-965 connector, `LiveDot` is the state dot and `conceptIcon` names the
// two group glyphs. Drawing the specimen out of those is the same geometry
// the web, the IDE, iOS and Android all paint, with no inline style and no
// second copy of Lucide's paths.

const ChevronDownIcon = conceptIcon(`ui-chevron-down`)
const ChevronRightIcon = conceptIcon(`ui-chevron-right`)
/** The two group glyphs, by CONCEPT: a workflow is a graph, a stack a chain. */
const WorkflowIcon = conceptIcon(`nav-workflows`)
const StackIcon = conceptIcon(`pr-stack`)

/** One row of the specimen — a group, or a run. */
type Row =
  | {
      kind: `group`
      depth: number
      icon: typeof WorkflowIcon
      name: string
      count: number
      /** A workflow's name links onward; a stack is not a place you can go. */
      link?: boolean
      expanded?: boolean
    }
  | {
      kind: `run`
      depth: number
      identifier: string
      title: string
      tone: `live` | `done` | `idle`
    }

const ROWS: Row[] = [
  // A workflow: its name is the only thing that links onward.
  { kind: `group`, depth: 0, icon: WorkflowIcon, name: `EXP-996 +5`, count: 3, link: true },
  { kind: `run`, depth: 1, identifier: `EXP-1048`, title: `Session tree (web)`, tone: `done` },
  { kind: `run`, depth: 1, identifier: `EXP-1049`, title: `Session tree (IDE)`, tone: `live` },
  {
    kind: `run`,
    depth: 1,
    identifier: `EXP-1050`,
    title: `Session tree (iOS + Android)`,
    tone: `idle`,
  },
  // A stack is a linear group: lowest first, and nowhere to go.
  { kind: `group`, depth: 0, icon: StackIcon, name: `Stacked pull requests`, count: 2 },
  { kind: `run`, depth: 1, identifier: `APP-41`, title: `Extract the merge queue`, tone: `done` },
  { kind: `run`, depth: 1, identifier: `APP-42`, title: `Retry a failed merge`, tone: `done` },
  // A parent run and its `sessions_start` child, one level deeper.
  { kind: `run`, depth: 0, identifier: `APP-88`, title: `Plan the release train`, tone: `done` },
  {
    kind: `run`,
    depth: 1,
    identifier: `APP-89`,
    title: `Bump the iOS build number`,
    tone: `done`,
  },
  // Folded: the group stays, its runs are gone.
  {
    kind: `group`,
    depth: 0,
    icon: WorkflowIcon,
    name: `REV2-12 +2`,
    count: 3,
    link: true,
    expanded: false,
  },
]

/** The ×4 indent rule, the same one every drawn list applies. */
function indent(depth: number): string {
  return `${TREE_BASE + depth * TREE_INDENT}px`
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
  island: () => {
    // The connector reads off the VISIBLE depths, exactly as the app's
    // `useSessionTreeRows` does — a folded subtree simply is not there.
    const guides = treeGuides(ROWS.map((row) => row.depth))
    return (
      <div className="flex w-[420px] max-w-full flex-col">
        {ROWS.map((row, index) => {
          const guide = guides[index]!
          if (row.kind === `group`) {
            const Icon = row.icon
            const expanded = row.expanded !== false
            const Fold = expanded ? ChevronDownIcon : ChevronRightIcon
            return (
              <ListRow
                key={`${row.name}-${index}`}
                interactive
                className="relative h-8 gap-1.5 py-0 pr-2 text-sm"
                style={{ paddingLeft: indent(row.depth) }}
              >
                <TreeGuides guide={guide} />
                <Fold className="size-3 shrink-0 text-muted-foreground" />
                <Icon className="size-3.5 shrink-0 text-muted-foreground" />
                <span
                  className={
                    row.link
                      ? `min-w-0 flex-1 truncate font-medium underline underline-offset-2`
                      : `min-w-0 flex-1 truncate font-medium`
                  }
                >
                  {row.name}
                </span>
                <span className="shrink-0 font-mono text-xs text-muted-foreground">
                  {row.count}
                </span>
              </ListRow>
            )
          }
          return (
            <ListRow
              key={row.identifier}
              interactive
              className="relative h-8 gap-2 py-0 pr-2 text-sm"
              style={{ paddingLeft: indent(row.depth) }}
            >
              <TreeGuides guide={guide} />
              <LiveDot
                tone={row.tone}
                ping={row.tone === `live`}
                className="size-1.5 shrink-0"
              />
              <span className="shrink-0 font-mono text-xs text-muted-foreground">
                {row.identifier}
              </span>
              <span className="min-w-0 flex-1 truncate">{row.title}</span>
            </ListRow>
          )
        })}
      </div>
    )
  },
}
