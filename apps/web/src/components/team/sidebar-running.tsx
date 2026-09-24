import type * as React from "react"
import { useMemo, useState } from "react"
import { useParams, useRouterState } from "@tanstack/react-router"
import { useLiveQuery } from "@tanstack/react-db"
import {
  AgentBrandMark,
  conceptIcon,
  getDeviceIcon,
  ListRow,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  Tooltip,
  TooltipContent,
  TooltipTrigger,
  TREE_BASE,
  TREE_INDENT,
  TreeGuides,
  treeGuides,
  type TreeGuide,
} from "@exp/ui"
import type { CodingSession, Device } from "@/db/schema"
import { deviceCollection } from "@/lib/collections"
import { sessionDisplayState } from "@/lib/coding-session-display"
import { sessionIdentity } from "@/lib/session-identity"
import { nestSessions, visibleTreeRows } from "@/lib/session-tree"
import { cn } from "@/lib/utils"
import { rowPrState, useSessionListRows, type SessionListRow } from "@/hooks/use-agents-data"
import { useMyLiveRuns } from "@/hooks/use-my-live-runs"
import { useOpenSession } from "@/hooks/use-open-session"

// EXP-923: RUNNING runs live in the SIDEBAR again.
//
// EXP-870 put every live run of mine on the work-tab strip; a handful of them
// filled the whole band, and a tab you could not close made the strip feel
// like a dashboard. The strip is back to being work you opened, and what is
// RUNNING is a section of the main menu — under Boards, nested by
// `parent_session_id` with the EXP-965 connector, hidden entirely when nothing
// runs (no empty copy, the desktop's `RunningSessionsSection` rule).
//
// The row is deliberately NOT the session-list row: one line, the AGENT's
// brand mark instead of a state dot (with a small amber badge when the run
// wants you), identifier + title, and the HOST DEVICE's icon pinned at the
// trailing edge. No status line, no "started ago", no ping — the sidebar says
// WHAT is running and WHERE, the run's own page says how it is going.
//
// Clicking a row navigates with the `running` origin, which creates no work
// tab (`lib/work-tabs.ts` `TABLESS_ORIGIN`) and keeps the main menu up.

const ChevronDownIcon = conceptIcon(`ui-chevron-down`)
const ChevronRightIcon = conceptIcon(`ui-chevron-right`)

const EMPTY_COLLAPSED: ReadonlySet<string> = new Set<string>()

/** One row of the section, joined and nested. */
export interface RunningRow {
  row: SessionListRow
  depth: number
  hasChildren: boolean
  guide: TreeGuide
  /** The device row behind `session.device_id`, for its icon. */
  device: Pick<Device, `icon` | `kind`> | undefined
  deviceName: string
  /** EXP-804/EXP-679: the run is parked on the person — the amber badge. */
  needsInput: boolean
  identifier: string | null
  subject: string
}

/** My live runs in the team, joined and nested — the section's model, shared
 *  by the expanded group and the compact rail's icon column. */
export function useMyRunningRows(
  teamId: string | undefined,
  currentUserId: string | undefined,
  collapsed: ReadonlySet<string> = EMPTY_COLLAPSED
): RunningRow[] {
  const { runs } = useMyLiveRuns(teamId, currentUserId, 30_000)
  const rows = useSessionListRows(teamId, runs)
  const { data: deviceRows } = useLiveQuery(
    (query) => (teamId ? query.from({ d: deviceCollection }) : undefined),
    [teamId]
  )
  return useMemo(() => {
    const byId = new Map(rows.map((row) => [row.session.id, row]))
    const devices = (deviceRows ?? []) as Device[]
    const deviceOf = (session: CodingSession) => {
      if (!session.deviceId) return undefined
      const matches = devices.filter((d) => d.deviceId === session.deviceId)
      return matches.find((d) => d.userId === session.userId) ?? matches[0]
    }
    const tree = visibleTreeRows(
      nestSessions(rows.map((row) => row.session)),
      collapsed
    )
    const guides = treeGuides(tree.map((entry) => entry.depth))
    return tree.flatMap((entry, index) => {
      const row = byId.get(entry.session.id)
      if (!row) return []
      const identity = sessionIdentity(row)
      const state = sessionDisplayState(
        row.session,
        rowPrState(row.session, row.issue)
      )
      return [
        {
          row,
          depth: entry.depth,
          hasChildren: entry.hasChildren,
          guide: guides[index]!,
          device: deviceOf(row.session),
          deviceName: row.device.label || row.session.deviceLabel || `Desktop`,
          needsInput: state === `needs_input`,
          identifier: identity.identifier,
          subject: identity.subject,
        } satisfies RunningRow,
      ]
    })
  }, [rows, deviceRows, collapsed])
}

/** Which run the app is SHOWING right now — a sidebar row lights up on either
 *  face of it, the run's own page and its issue's. */
function useShownRun(): { runId: string | null; issueIdentifier: string | null } {
  const pathname = useRouterState({ select: (s) => s.location.pathname })
  const { issueIdentifier } = useParams({ strict: false }) as {
    issueIdentifier?: string
  }
  return {
    runId: /\/sessions\/([^/]+)$/.exec(pathname)?.[1] ?? null,
    issueIdentifier: issueIdentifier ?? null,
  }
}

function isShown(
  entry: RunningRow,
  shown: { runId: string | null; issueIdentifier: string | null }
): boolean {
  if (shown.runId && entry.row.session.id === shown.runId) return true
  return Boolean(
    shown.issueIdentifier &&
      entry.row.issue?.identifier === shown.issueIdentifier
  )
}

/** The brand mark with its amber "wants you" badge — the row's lead and the
 *  rail button's whole content. */
function RunningMark({
  agent,
  needsInput,
  className,
}: {
  agent: string | null
  needsInput: boolean
  className?: string
}) {
  return (
    <span
      className={cn(`relative flex size-3.5 items-center justify-center`, className)}
    >
      <AgentBrandMark agent={agent} className="size-3.5" />
      {needsInput && (
        <span
          aria-hidden
          className="absolute -top-0.5 -right-0.5 size-1.5 rounded-full bg-yellow-400 ring-2 ring-sidebar"
        />
      )}
    </span>
  )
}

/** EXP-923: the expanded sidebar's Running group — hidden with nothing live. */
export function SidebarRunningSection({
  teamId,
  currentUserId,
}: {
  teamId: string | undefined
  currentUserId: string | undefined
}) {
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(
    () => new Set<string>()
  )
  const entries = useMyRunningRows(teamId, currentUserId, collapsed)
  const shown = useShownRun()
  const openSession = useOpenSession()
  if (entries.length === 0) return null
  const toggle = (sessionId: string) =>
    setCollapsed((current) => {
      const next = new Set(current)
      if (!next.delete(sessionId)) next.add(sessionId)
      return next
    })
  return (
    <SidebarGroup data-testid="sidebar-running">
      {/* EXP-1022: the main menu's plain group label, like Pinned and Boards
          above it — the filled group band belongs to LISTS, not the menu. */}
      <SidebarGroupLabel>Running</SidebarGroupLabel>
      <SidebarGroupContent>
        {/* Gapless (EXP-965: nothing for a connector to bridge). */}
        <div className="flex flex-col">
          {entries.map((entry) => {
            const session = entry.row.session
            const DeviceIcon = getDeviceIcon(entry.device ?? {})
            const expanded = !collapsed.has(session.id)
            return (
              <ListRow
                key={session.id}
                interactive
                active={isShown(entry, shown)}
                onClick={() =>
                  openSession(session, { origin: { kind: `running` } })
                }
                className="relative h-8 gap-1.5 py-0 pr-2 text-sm"
                style={{
                  paddingLeft: `${TREE_BASE + entry.depth * TREE_INDENT}px`,
                }}
                data-testid={`sidebar-running-${session.id}`}
              >
                <TreeGuides guide={entry.guide} />
                {entry.hasChildren && (
                  <span
                    role="button"
                    tabIndex={-1}
                    aria-label={
                      expanded ? `Collapse child runs` : `Expand child runs`
                    }
                    className="flex shrink-0 items-center justify-center text-muted-foreground hover:text-foreground"
                    onClick={(event) => {
                      event.stopPropagation()
                      toggle(session.id)
                    }}
                  >
                    {expanded ? (
                      <ChevronDownIcon className="size-3" />
                    ) : (
                      <ChevronRightIcon className="size-3" />
                    )}
                  </span>
                )}
                <RunningMark
                  agent={session.agent}
                  needsInput={entry.needsInput}
                />
                {entry.identifier && (
                  <span className="shrink-0 font-mono text-xs text-muted-foreground">
                    {entry.identifier}
                  </span>
                )}
                <span className="min-w-0 flex-1 truncate">{entry.subject}</span>
                {/* Fixed, never truncated: WHERE the run is. */}
                <Tooltip>
                  <TooltipTrigger asChild>
                    <span className="flex shrink-0 items-center justify-center">
                      <DeviceIcon className="size-3.5 text-muted-foreground" />
                    </span>
                  </TooltipTrigger>
                  <TooltipContent side="right">
                    {entry.deviceName}
                  </TooltipContent>
                </Tooltip>
              </ListRow>
            )
          })}
        </div>
      </SidebarGroupContent>
    </SidebarGroup>
  )
}

/** EXP-923: the compact rail's Running column — one 32px button per live run
 *  under the board icons, children straight under their parents (a 48px
 *  column has no room to indent). The rail owns the button chrome, so it
 *  hands one in, exactly like `SidebarPinnedIcons`. */
export function SidebarRunningIcons({
  teamId,
  currentUserId,
  renderItem,
  separator,
}: {
  teamId: string | undefined
  currentUserId: string | undefined
  renderItem: (item: {
    key: string
    label: string
    active: boolean
    onClick: () => void
    icon: React.ReactNode
  }) => React.ReactNode
  separator?: React.ReactNode
}) {
  const entries = useMyRunningRows(teamId, currentUserId)
  const shown = useShownRun()
  const openSession = useOpenSession()
  if (entries.length === 0) return null
  return (
    <>
      {separator}
      {entries.map((entry) => {
        const session = entry.row.session
        return renderItem({
          key: session.id,
          // The expanded row's whole label — identifier AND title.
          label: [entry.identifier, entry.subject].filter(Boolean).join(` `),
          active: isShown(entry, shown),
          onClick: () => openSession(session, { origin: { kind: `running` } }),
          icon: (
            <RunningMark
              agent={session.agent}
              needsInput={entry.needsInput}
            />
          ),
        })
      })}
    </>
  )
}
