import {
  sessionCard,
  type SessionState,
  type TreeSession,
} from "@exp/domain-contract/session-tree"
import { type RunMarkState, SessionRow, treeGuides } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1248 → EXP-1293: THE session row, the REAL `@exp/ui` SessionRow, fed
// by the contract's `sessionCard` (the same state the session card shows).
// The rail's Running rows: the agent mark, the issue identifier, the title.
// EXP-1323: a live tree nests its children (depth + TreeGuides), a bold
// title = unread.

const NOW = `2026-10-13T12:00:00.000Z`

const session = (over: Partial<TreeSession> & { id: string }): TreeSession => ({
  teamId: `t-acme`,
  parentId: null,
  rootId: over.id,
  userId: `u-alex`,
  status: `running`,
  busy: false,
  needsInput: false,
  blocked: null,
  shared: true,
  title: null,
  guide: null,
  startedAt: `2026-10-13T11:40:00.000Z`,
  endedAt: null,
  createdAt: `2026-10-13T11:40:00.000Z`,
  updatedAt: `2026-10-13T11:58:00.000Z`,
  ...over,
})

/** The run mark per card state (the status row's mapping). */
const MARK: Record<SessionState, RunMarkState | undefined> = {
  queued: undefined,
  working: `working`,
  // EXP-1374: background tasks still hold the slot (the working mark); an
  // interruption is the bare mark.
  background: `working`,
  waiting: undefined,
  needs_you: `needs_input`,
  interrupted: undefined,
  blocked: undefined,
  ended: `ended`,
}

interface SpecimenRow {
  depth: number
  identifier: string | null
  session: TreeSession
  unread?: boolean
}

const ROWS: readonly SpecimenRow[] = [
  {
    depth: 0,
    identifier: `APP-5`,
    session: session({ id: `s1`, title: `Reduce cold start below 800 ms`, needsInput: true }),
    unread: true,
  },
  {
    depth: 1,
    identifier: null,
    session: session({
      id: `s2`,
      parentId: `s1`,
      rootId: `s1`,
      title: `Profile the launch path`,
      busy: true,
    }),
  },
  {
    depth: 2,
    identifier: null,
    session: session({ id: `s5`, parentId: `s2`, rootId: `s1`, title: `Trace the splash screen` }),
    unread: true,
  },
  {
    depth: 0,
    identifier: `APP-14`,
    session: session({ id: `s3`, title: `Group board issues by status`, busy: true }),
  },
  {
    depth: 0,
    identifier: null,
    session: session({
      id: `s4`,
      title: `Pre-cleanup release`,
      status: `ended`,
      endedAt: `2026-10-13T11:50:00.000Z`,
    }),
  },
]

function SessionRowSpecimen() {
  const guides = treeGuides(ROWS.map((row) => row.depth))
  return (
    <div className="flex w-[300px] max-w-full flex-col gap-1 rounded-lg bg-sidebar p-2">
      <div className="px-2 py-1 text-xs text-muted-foreground">Running</div>
      {ROWS.map((row, index) => {
        const card = sessionCard(row.session, row.session.title ?? ``, NOW)
        return (
          <SessionRow
            key={row.session.id}
            size="small"
            agent="claude"
            markState={MARK[card.state]}
            identifier={row.identifier}
            title={card.name}
            unread={row.unread}
            depth={row.depth}
            guide={guides[index]}
            active={index === 0}
            ringClassName="ring-sidebar"
            onClick={() => {}}
          />
        )
      })}
    </div>
  )
}

export const entry: StyleguideEntry = {
  id: `session-row`,
  section: `special`,
  owner: `EXP-1248`,
  title: `Session row`,
  blurb: `ONE list item for a session, nested by the session tree (a child session under its root). Anatomy ×4: [tree guides][session mark at 12 + 14·depth][mono id · title]. The mark is the agent's brand mark, its state the session card's (\`sessionCard\`: Claude's spark while it works, a badge when it needs you, dimmed once ended). EXP-1293: no device. small = one line, 32px (the rail's Running). EXP-1323: a live tree's children nest under the root in the Threads list and the rail (depth + TreeGuides, 14px a level, the 3px corner tee; a child opens that session); a bold title = an agent reply the viewer has not read.`,
  status: {
    web: {
      state: `ok`,
      symbol: `SessionRow`,
      file: `packages/ui/src/session-row.tsx`,
      note: `the rail's Running (team/sidebar-running.tsx); state = sessionCard (lib/session-tree)`,
    },
    desktop: {
      state: `ok`,
      symbol: `run_rows::run_row`,
      file: `apps/desktop/crates/ui/src/run_rows.rs`,
      note: `small = the rail Running (sidebar.rs), big = Recent and the action Sessions; caption domain/src/session_row.rs`,
    },
    ios: {
      state: `ok`,
      symbol: `SessionRow`,
      file: `apps/ios/ExpUI/Sources/SessionRow.swift`,
      note: `the home's Running band and the Threads tab (ThreadRowLabel, UI/MyWork/ThreadsView.swift)`,
    },
    android: {
      state: `ok`,
      symbol: `ThreadListRow`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/threads/ThreadsScreen.kt`,
      note: `the home's Running band and the Threads tab`,
    },
  },
  island: () => <SessionRowSpecimen />,
}
