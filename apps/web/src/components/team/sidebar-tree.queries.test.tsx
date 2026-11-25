import { act, render } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { SidebarProvider, TooltipProvider } from "@exp/ui"
import type { SidebarEntry, SidebarRow } from "@exp/domain-contract/sidebar-folders"

// #PERF-13: the every-team sidebar (#EXP-2027) read every session list once
// PER TEAM SECTION, on every synced row (about 7 session live queries per
// team: with 5 teams, ~35 per streamed message). Locked: one synced session
// write re-runs a FIXED number of session live queries, whatever the number
// of teams, and no team section renders again for a row none of its lists
// shows. `useLiveQuery` is a counting fake over one store version.

const db = vi.hoisted(() => ({
  version: 0,
  listeners: new Set<() => void>(),
  data: {} as Record<string, unknown[]>,
  /** Session live queries run (each `useLiveQuery` over the sessions). */
  sessionQueries: 0,
  sectionRenders: 0,
}))

function bump(mutate: () => void) {
  mutate()
  db.version += 1
  for (const listener of db.listeners) listener()
}

vi.mock(`@tanstack/react-db`, async () => {
  const React = await import(`react`)
  const op = (...args: unknown[]) => args
  const subscribe = (listener: () => void) => {
    db.listeners.add(listener)
    return () => db.listeners.delete(listener)
  }
  return {
    and: op,
    or: op,
    eq: op,
    inArray: op,
    isNull: op,
    not: op,
    gt: op,
    useLiveQuery: (fn: unknown) => {
      React.useSyncExternalStore(subscribe, () => db.version)
      let from: string | null = null
      const chain: Record<string, unknown> = {}
      for (const name of [`where`, `select`, `orderBy`, `limit`, `join`, `fn`]) chain[name] = () => chain
      const query = {
        from: (source: Record<string, { __name: string }>) => {
          from = Object.values(source)[0]?.__name ?? null
          return chain
        },
      }
      const built = typeof fn === `function` ? (fn as (q: typeof query) => unknown)(query) : fn
      if (built === undefined || from === null) return { data: undefined, isReady: true }
      if (from === `sessionCollection`) db.sessionQueries += 1
      return { data: db.data[from] ?? [], isReady: true }
    },
    createLiveQueryCollection: () => ({ status: `ready` }),
  }
})
vi.mock(`@/lib/collections`, () => {
  const named = (name: string) => ({ __name: name, get: () => undefined, utils: {} })
  return {
    sessionCollection: named(`sessionCollection`),
    agentCollection: named(`agentCollection`),
    teamCollection: named(`teamCollection`),
    sessionIssueCollection: named(`sessionIssueCollection`),
    issueCollection: named(`issueCollection`),
    sessionReadsCollection: named(`sessionReadsCollection`),
    sidebarLayoutCollection: named(`sidebarLayoutCollection`),
    pullRequestCollection: named(`pullRequestCollection`),
    messageCollection: named(`messageCollection`),
    appCollection: named(`appCollection`),
    deviceCollection: named(`deviceCollection`),
  }
})
vi.mock(`@/lib/enet/sealed-open`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/enet/sealed-open")>()),
  openDigestMessages: (_id: string, messages: unknown[]) => messages,
}))
vi.mock(`@/lib/enet/use-key-holder`, () => ({ useKeyVersion: () => 0 }))
vi.mock(`@/lib/chat-store/client`, () => ({ recordMessages: () => {} }))
vi.mock(`@tanstack/react-router`, () => ({
  Link: ({ children }: { children: React.ReactNode }) => <a>{children}</a>,
  useNavigate: () => vi.fn(),
}))
vi.mock(`@/lib/trpc-client`, () => ({ trpc: {} }))
vi.mock(`@/hooks/use-open-new-draft`, () => ({ useOpenNewDraft: () => () => {} }))
vi.mock(`@/components/chat/chat-menu`, () => ({
  useAgentSends: () => null,
  useCanDeleteChat: () => false,
  useChatActions: () => ({ requestDelete: () => {}, dialog: null }),
}))
vi.mock(`@/components/team/sidebar-shared`, () => ({ NavCountBadge: () => null }))
vi.mock(`@/hooks/use-device-glyphs`, () => ({ useDeviceGlyphs: () => () => null }))
vi.mock(`@/hooks/use-team-data`, () => ({ useTeamBoards: () => [] }))
vi.mock(`@/hooks/use-apps`, () => ({ useApps: () => ({ apps: [] }) }))
vi.mock(`@/hooks/use-sidebar-folders`, () => ({
  useSidebarFolders: (_teamId: string, _section: string, ids: (entries: SidebarEntry[]) => string[]) => {
    db.sectionRenders += 1
    const items = ids([])
    return {
      rows: items.map((id): SidebarRow => ({ kind: `item`, id, depth: 0, group: false, name: null, open: false, count: 0 })),
      items,
      editingId: null,
      setEditingId: () => {},
      move: () => {},
      toggle: () => {},
      rename: () => {},
      ungroup: () => {},
      addFolder: () => {},
      dropSession: () => {},
    }
  },
}))

import { SidebarTeamsGroup, sidebarActive } from "@/components/team/sidebar-tree"
import type { Team } from "@/db/schema"

const ME = `u-me`

function seed(teamCount: number) {
  const teams = Array.from({ length: teamCount }, (_, t) => ({ id: `t${t}`, slug: `team-${t}`, name: `Team ${t}`, iconUrl: null }))
  const sessions = teams.flatMap((team) =>
    Array.from({ length: 40 }, (_, n) => ({
      id: `${team.id}-s${n}`,
      teamId: team.id,
      rootId: `${team.id}-s${n}`,
      parentId: null,
      userId: ME,
      status: `idle`,
      shared: true,
      title: `Chat ${n}`,
      createdAt: `2026-11-20T10:00:00.000Z`,
      updatedAt: `2026-11-20T10:00:00.000Z`,
      digest: null,
    }))
  )
  db.data = {
    teamCollection: teams,
    sessionCollection: sessions,
    agentCollection: [],
    sessionIssueCollection: [],
    issueCollection: [],
    sessionReadsCollection: [],
  }
  return teams as unknown as Team[]
}

/** Session live queries and section renders of ONE synced session write. */
function oneWrite(teamCount: number): { queries: number; sections: number } {
  const teams = seed(teamCount)
  const active = sidebarActive(`/home`)
  const owners = new Set<string>()
  const needsYou = new Map<string, number>()
  const chatRows = new Map()
  const view = render(
    <TooltipProvider>
      <SidebarProvider>
        <SidebarTeamsGroup
          teams={teams}
          owners={owners}
          needsYou={needsYou}
          chatRows={chatRows}
          me={ME}
          active={active}
          onNewTeam={() => {}}
          onJoinTeam={() => {}}
        />
      </SidebarProvider>
    </TooltipProvider>
  )
  db.sessionQueries = 0
  db.sectionRenders = 0
  // A streamed reply rewrites one session's digest in the first team.
  act(() =>
    bump(() => {
      db.data.sessionCollection = (db.data.sessionCollection as { id: string }[]).map((row) =>
        row.id === `t0-s3` ? { ...row, updatedAt: `2026-11-22T10:00:00.000Z`, digest: { agentAt: `2026-11-22T10:00:00.000Z`, agentTimes: [`2026-11-22T10:00:00.000Z`] } } : row
      )
    })
  )
  const out = { queries: db.sessionQueries, sections: db.sectionRenders }
  view.unmount()
  return out
}

describe(`#PERF-13 the every-team sidebar's work per synced row`, () => {
  it(`runs a fixed number of session live queries, however many teams`, () => {
    const one = oneWrite(1)
    const five = oneWrite(5)
    // Budget: the group's one read (sessions, digests, agents, titles).
    // Per-team reads were ~7 per team (5 teams: ~35).
    expect(five.queries).toBeLessThanOrEqual(8)
    expect(five.queries).toBe(one.queries)
  })

  it(`renders no team section for a row none of its lists moved`, () => {
    expect(oneWrite(5).sections).toBe(0)
  })
})
