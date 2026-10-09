import type * as React from "react"
import { useMemo } from "react"
import { useParams, useRouterState } from "@tanstack/react-router"
import { useLiveQuery } from "@tanstack/react-db"
import {
  AgentRunMark,
  type RunMarkState,
  getDeviceIcon,
  SessionRow,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  type TreeGuide,
} from "@exp/ui"
import type { CodingSession, Device } from "@/db/schema"
import { deviceCollection } from "@/lib/collections"
import { sessionDisplayState } from "@/lib/coding-session-display"
import { sessionIdentity } from "@/lib/session-identity"
import { sessionDeviceRow, useSessionTreeRows } from "@/components/session-tree"
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
// The row is SessionRow at size small: one line, the AGENT's run mark (with a
// small badge when the run wants you), identifier + title, and the HOST
// DEVICE's icon pinned at the trailing edge. No caption: the sidebar says
// WHAT is running and WHERE, the run's own page says how it is going.
//
// Clicking a row navigates with the `running` origin, which creates no work
// tab (`lib/work-tabs.ts` `TABLESS_ORIGIN`) and keeps the main menu up.
//
// EXP-996: and the nesting is the whole `sessionTree`, resumes collapsed.
// EXP-1248: the row is `@exp/ui` SessionRow, size small, children always
// shown (no fold), so a parent's mark sits exactly where its child's elbow
// points.

/** One row of the section, nested: a live run. */
export interface RunningSessionEntry {
  /** `sessionTreeNodeKey`. */
  key: string
  depth: number
  hasChildren: boolean
  guide: TreeGuide
  row: SessionListRow
  /** The device row behind `session.device_id`, for its icon. */
  device: Pick<Device, `icon` | `kind`> | undefined
  deviceName: string
  /** EXP-804/EXP-679: the run is parked on the person — the amber badge. */
  state: RunMarkState | undefined
  identifier: string | null
  subject: string
}

/** My live runs in the team, joined and nested — the section's model, shared
 *  by the expanded group and the compact rail's icon column. */
export function useMyRunningRows(
  teamId: string | undefined,
  currentUserId: string | undefined
): RunningSessionEntry[] {
  const { runs } = useMyLiveRuns(teamId, currentUserId, 30_000)
  const rows = useSessionListRows(teamId, runs)
  const tree = useSessionTreeRows(rows)
  const { data: deviceRows } = useLiveQuery(
    (query) => (teamId ? query.from({ d: deviceCollection }) : undefined),
    [teamId]
  )
  return useMemo(() => {
    const devices = (deviceRows ?? []) as Device[]
    const deviceOf = (session: CodingSession) => sessionDeviceRow(session, devices)
    return tree.flatMap(({ flat, row, guide }): RunningSessionEntry[] => {
      if (!row) return []
      const identity = sessionIdentity(row)
      const state = sessionDisplayState(
        row.session,
        rowPrState(row.session, row.issue)
      )
      return [
        {
          key: flat.key,
          depth: flat.depth,
          hasChildren: flat.hasChildren,
          guide,
          row,
          device: deviceOf(row.session),
          deviceName: row.device.label || row.session.deviceLabel || `Desktop`,
          // EXP-1184: what the run is doing — the mark's spark or badge. A
          // paused run (offline host) wears the bare mark.
          state: row.paused ? undefined : state,
          identifier: identity.identifier,
          subject: identity.subject,
        } satisfies RunningSessionEntry,
      ]
    })
  }, [tree, deviceRows])
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
  entry: RunningSessionEntry,
  shown: { runId: string | null; issueIdentifier: string | null }
): boolean {
  if (shown.runId && entry.row.session.id === shown.runId) return true
  return Boolean(
    shown.issueIdentifier &&
      entry.row.issue?.identifier === shown.issueIdentifier
  )
}

/** The run's mark — the working spark, or the brand mark with its state
 *  badge (EXP-1184) — the row's lead and the rail button's whole content. EXP-1162: `@exp/ui` `AgentRunMark`, the very
 *  mark the Work face strip's Run tab wears. */
const RunningMark = AgentRunMark

/** EXP-923: the expanded sidebar's Running group — hidden with nothing live. */
export function SidebarRunningSection({
  teamId,
  currentUserId,
}: {
  teamId: string | undefined
  currentUserId: string | undefined
}) {
  const entries = useMyRunningRows(teamId, currentUserId)
  const shown = useShownRun()
  const openSession = useOpenSession()
  if (entries.length === 0) return null
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
            return (
              <SessionRow
                key={entry.key}
                size="small"
                agent={session.agent}
                markState={entry.state}
                identifier={entry.identifier}
                title={entry.subject}
                depth={entry.depth}
                guide={entry.guide}
                deviceIcon={getDeviceIcon(entry.device ?? {})}
                deviceName={entry.deviceName}
                active={isShown(entry, shown)}
                ringClassName="ring-sidebar"
                onClick={() =>
                  openSession(session, { origin: { kind: `running` } })
                }
                data-testid={`sidebar-running-${session.id}`}
              />
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
            <RunningMark agent={session.agent} state={entry.state} />
          ),
        })
      })}
    </>
  )
}
