import { useEffect, useMemo, useRef, useState } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { Link } from "@tanstack/react-router"
import type { CodingSession, Issue } from "@/db/schema"
import { codingSessionCollection, issueCollection } from "@/lib/collections"
import { useTeamBoards } from "@/hooks/use-team-data"
import { useAgentsData, type AgentSessionRow } from "@/hooks/use-agents-data"
import { AgentSessionView } from "@/components/agent-session"
import { sessionDisplayState } from "@/components/issue-coding-rows"
import { cn } from "@/lib/utils"
import { Button } from "@/components/ui/button"
import { useAgentDock } from "@/components/agent-dock/agent-dock-provider"
import { retainSteerSessions } from "@/lib/steer-session-store"
import { useIsMobile } from "@/hooks/use-mobile"
import {
  clampAgentDockHeight,
  readAgentDockHeight,
  writeAgentDockHeight,
} from "@/lib/agent-dock-height"
import { MOTION_DURATION_MS } from "@/lib/motion"
import { useExitPresence } from "@/hooks/use-exit-presence"
import { useWheelContainment } from "@/hooks/use-wheel-containment"

// The global agent-coding dock (EXP-106) — an IDE-style bottom strip of the
// current user's OWN running sessions (EXP-312: live sessions are owner-only;
// teammates' runs surface as status badges elsewhere, never here), with at
// most one expanded live viewer. It's the SOLE mount point for
// AgentSessionView; issue detail and the Agents page only call openDock().
// Renders nothing when there's nothing to show.
// Desktop-only chrome (EXP-193): on mobile there is no bottom strip — an
// opened session takes over the viewport like the native apps' pushed
// Agent-session screen, and running sessions are reached from the Agents tab
// (green dot) or the issue's Watch button instead.
// EXP-523: the panel SLIDES. Opening and collapsing used to be a bare
// mount/unmount, so the whole page jumped by the panel height in one frame.
// `useExitPresence` keeps the outgoing row mounted for one `standard`
// duration while the height animates back to 0 (mobile: while the takeover
// slides off the bottom), and the enter runs from the closed state on the
// frame after mount.

function RunningDot() {
  return (
    <span className="relative flex size-2 shrink-0">
      <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-emerald-400 opacity-60" />
      <span className="relative inline-flex size-2 rounded-full bg-emerald-500" />
    </span>
  )
}

export function AgentDock({
  teamId,
  teamSlug,
  currentUserId,
}: {
  teamId: string
  teamSlug: string
  currentUserId: string
}) {
  const dock = useAgentDock()
  // EXP-312: only the caller's own sessions get dock tabs (a teammate's
  // session can't be opened live, the ticket mint refuses non-owners), and
  // useAgentsData already scopes its rows to the caller.
  const { running } = useAgentsData(teamId, currentUserId)
  const boards = useTeamBoards(teamId)
  const isMobile = useIsMobile()

  const expandedId = dock?.expandedSessionId ?? null

  // Fullscreen (EXP-184) lives here — outside the per-session `key` remount,
  // so switching dock tabs keeps fullscreen; any collapse exits it.
  const [fullscreen, setFullscreen] = useState(false)
  useEffect(() => {
    if (!expandedId) setFullscreen(false)
  }, [expandedId])

  // EXP-619: the expanded panel floats over the page, which keeps scrolling
  // underneath it — a wheel gesture the terminal cannot use must die here
  // rather than slide the issue list behind it. Only the panel is guarded;
  // the bare tab strip sits at the bottom edge of every page.
  const containWheel = useWheelContainment<HTMLDivElement>()

  // Drag-to-resize (EXP-234) — the panel height persists per device, like the
  // IDE terminal dock. `max-h-[85vh]` on the panel keeps an oversized saved
  // value usable on a smaller window without rewriting it.
  const [panelHeight, setPanelHeight] = useState(readAgentDockHeight)
  const [resizing, setResizing] = useState(false)
  const resizeRef = useRef<{
    startY: number
    startHeight: number
    latestHeight: number | null
  } | null>(null)

  const beginPanelResize = (event: React.PointerEvent<HTMLDivElement>) => {
    event.preventDefault()
    resizeRef.current = {
      startY: event.clientY,
      startHeight: panelHeight,
      latestHeight: null,
    }
    setResizing(true)
    event.currentTarget.setPointerCapture(event.pointerId)
  }

  const movePanelResize = (event: React.PointerEvent<HTMLDivElement>) => {
    const drag = resizeRef.current
    if (!drag) return
    event.preventDefault()
    const next = clampAgentDockHeight(
      drag.startHeight + (drag.startY - event.clientY),
      window.innerHeight
    )
    drag.latestHeight = next
    setPanelHeight(next)
  }

  const endPanelResize = () => {
    const drag = resizeRef.current
    if (!drag) return
    resizeRef.current = null
    setResizing(false)
    if (drag.latestHeight !== null) writeAgentDockHeight(drag.latestHeight)
  }

  // EXP-621: the reaper for background steer connections — stores for the
  // user's running sessions (and the expanded one) are kept alive so a
  // collapsed or re-opened session resumes instantly; anything else ages out
  // inside the registry (grace-delayed, so a transiently empty live query
  // can't kill a socket).
  useEffect(() => {
    const keep = new Set(running.map((row) => row.session.id))
    if (expandedId) keep.add(expandedId)
    retainSteerSessions(keep)
  }, [running, expandedId])

  // Query the expanded session by id in ANY status, so a session that flips to
  // `ended` while expanded stays visible until the user collapses it.
  const { data: expandedRows } = useLiveQuery(
    (query) =>
      expandedId
        ? query
            .from({ s: codingSessionCollection })
            .where(({ s }) => eq(s.id, expandedId))
        : undefined,
    [expandedId]
  )
  const expandedSession =
    (((expandedRows ?? []) as CodingSession[])[0] ?? null) || null

  // Resolve the expanded session's issue when it isn't among the running rows
  // (i.e. it ended while expanded) — the running rows already carry the join.
  const runningById = useMemo(
    () => new Map(running.map((row) => [row.session.id, row])),
    [running]
  )
  const needsIssueJoin = Boolean(
    expandedSession &&
    expandedSession.issueId &&
    !runningById.has(expandedSession.id)
  )
  const { data: expandedIssueRows } = useLiveQuery(
    (query) =>
      needsIssueJoin && expandedSession?.issueId
        ? query
            .from({ i: issueCollection })
            .where(({ i }) => eq(i.id, expandedSession.issueId))
        : undefined,
    [needsIssueJoin, expandedSession?.issueId]
  )

  const expandedRow: AgentSessionRow | null = useMemo(() => {
    if (!expandedSession) return null
    const existing = runningById.get(expandedSession.id)
    if (existing) return existing
    const issue = expandedSession.issueId
      ? ((expandedIssueRows ?? [])[0] as Issue | undefined)
      : undefined
    const board = issue ? boards.find((p) => p.id === issue.boardId) : undefined
    return {
      session: expandedSession,
      issue,
      board,
      user: undefined,
      batchPrIssue: undefined,
      // A row held open past its live listing (ended while expanded): the
      // snapshot label suffices and nothing there is "paused" — the session
      // view resolves the live device itself.
      device: { label: expandedSession.deviceLabel, online: null },
      paused: false,
    }
  }, [expandedSession, runningById, expandedIssueRows, boards])

  // The expanded row vanished entirely (hard-deleted) — auto-collapse.
  useEffect(() => {
    if (expandedId && expandedRows && expandedSession === null) {
      dock?.collapseDock()
    }
  }, [expandedId, expandedRows, expandedSession])

  // EXP-523: `panelRow` is `expandedRow` while open, then the OUTGOING row for
  // one exit animation; `panelOpen` is what the CSS reads. Keyed on the
  // session id because `expandedRow` is a fresh object every render.
  const { value: panelRow, open: panelOpen } = useExitPresence(
    expandedRow,
    expandedRow?.session.id ?? null,
    MOTION_DURATION_MS.standard
  )

  if (running.length === 0 && !panelRow) return null

  // Mobile (EXP-193): no IDE-style bottom terminal strip. An opened session
  // is a full-viewport takeover (native Agent-session parity) — its collapse
  // chevron dismisses back to wherever it was opened from; nothing renders
  // while no session is open. z-40 covers the z-[35] floating tab bar while
  // staying under every z-50 overlay (kill-confirm dialog etc.).
  if (isMobile) {
    if (!panelRow) return null
    return (
      <div
        ref={containWheel}
        className={cn(
          `fixed inset-0 z-40 bg-app-gradient pb-[env(safe-area-inset-bottom)]`,
          // EXP-523: the takeover rises from the bottom edge it was opened
          // from and leaves the same way — the native apps' pushed-screen
          // gesture, without a navigation controller. `fill-mode-forwards`
          // pins the exit at its end state so the panel cannot flash back to
          // full opacity between the animation ending and the unmount.
          panelOpen
            ? `motion-safe:animate-in motion-safe:slide-in-from-bottom motion-safe:fade-in-0 duration-standard ease-decelerate`
            : `motion-safe:animate-out motion-safe:slide-out-to-bottom motion-safe:fade-out-0 duration-standard ease-accelerate fill-mode-forwards`
        )}
      >
        <AgentSessionView
          key={panelRow.session.id}
          session={panelRow.session}
          currentUserId={currentUserId}
          title={<SessionTitle row={panelRow} teamSlug={teamSlug} />}
          onCollapse={() => dock?.collapseDock()}
        />
      </div>
    )
  }

  const tabs =
    panelRow && !runningById.has(panelRow.session.id)
      ? [...running, panelRow]
      : running

  return (
    <div
      className={cn(
        // z-40 covers the layout while staying under every z-50 overlay
        // (dialogs, dropdowns) so kill-confirm etc. still stack above.
        // Fullscreen takeover repaints the gradient (opaque); the docked bar
        // is a blurred glass strip over whatever scrolls beneath it.
        fullscreen && panelRow
          ? `fixed inset-0 z-40 flex flex-col bg-app-gradient border-border`
          : `sticky bottom-0 z-30 border-t border-border/60 glass-chrome-bottom`,
        // EXP-523: the strip itself only exists once there IS a session, so it
        // appears mid-session out of nowhere. Run once on mount (the class is
        // static, so toggling fullscreen cannot restart it).
        `motion-safe:animate-in motion-safe:slide-in-from-bottom-2 motion-safe:fade-in-0 duration-standard ease-decelerate`
      )}
    >
      {panelRow && (
        <div
          ref={containWheel}
          className={cn(
            fullscreen ? `min-h-0 flex-1` : `relative max-h-[85vh]`,
            // EXP-523: the height IS the animation. It must be off while the
            // user drags the grabber — that writes `height` on every
            // pointermove, and a transition would lag the pointer by a whole
            // duration. Fullscreen is flex-sized, so nothing to animate there.
            !fullscreen &&
              !resizing &&
              `overflow-hidden transition-[height] duration-standard ease-standard motion-reduce:transition-none`
          )}
          style={
            fullscreen ? undefined : { height: panelOpen ? panelHeight : 0 }
          }
        >
          {!fullscreen && panelOpen && (
            <div
              className="group absolute inset-x-0 -top-1 z-10 h-2 cursor-row-resize touch-none"
              role="separator"
              aria-orientation="horizontal"
              aria-label="Resize agent panel"
              onPointerDown={beginPanelResize}
              onPointerMove={movePanelResize}
              onPointerUp={endPanelResize}
              onPointerCancel={endPanelResize}
            >
              <div
                className={cn(
                  `absolute inset-x-0 top-[3px] h-0.5 transition-colors group-hover:bg-primary/50`,
                  resizing && `bg-primary/70`
                )}
              />
            </div>
          )}
          {/* Pinned to the panel's RESTING height so the session view's own
              layout (and its virtualised feed) is not re-measured on every
              frame of the slide — the wrapper clips it instead. */}
          <div
            className="h-full"
            style={fullscreen ? undefined : { height: panelHeight }}
          >
            <AgentSessionView
              key={panelRow.session.id}
              session={panelRow.session}
              currentUserId={currentUserId}
              title={<SessionTitle row={panelRow} teamSlug={teamSlug} />}
              onCollapse={() => {
                setFullscreen(false)
                dock?.collapseDock()
              }}
              isFullscreen={fullscreen}
              onToggleFullscreen={() => setFullscreen((f) => !f)}
            />
          </div>
        </div>
      )}
      <div className="flex h-9 items-center overflow-x-auto">
        {tabs.map((row) => (
          <DockTab
            key={row.session.id}
            row={row}
            expanded={expandedId === row.session.id}
            onClick={() =>
              expandedId === row.session.id
                ? dock?.collapseDock()
                : dock?.openDock(row.session.id)
            }
          />
        ))}
      </div>
    </div>
  )
}

function SessionTitle({
  row,
  teamSlug,
}: {
  row: AgentSessionRow
  teamSlug: string
}) {
  const { session, issue, board } = row
  if (issue && board) {
    return (
      <Link
        to="/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier"
        params={{
          teamSlug,
          boardSlug: board.slug,
          issueIdentifier: issue.identifier,
        }}
        className="font-mono hover:underline"
      >
        {issue.identifier}
      </Link>
    )
  }
  // Issueless action/batch row, or an issue-scoped row whose issue hasn't
  // synced yet — action runs keep their name snapshot even if the action
  // was deleted (EXP-253).
  if (!session.issueId) {
    return session.actionName ? (
      <span className="truncate">{session.actionName}</span>
    ) : (
      <span className="font-mono">Batch</span>
    )
  }
  return (
    <span className="font-mono">{issue?.identifier ?? `Issue syncing…`}</span>
  )
}

function DockTab({
  row,
  expanded,
  onClick,
}: {
  row: AgentSessionRow
  expanded: boolean
  onClick: () => void
}) {
  const { session, issue, device, paused } = row
  // Action runs label their tab with the action-name snapshot (EXP-253).
  const label = issue?.identifier ?? session.actionName ?? `Batch`
  return (
    <Button
      variant="ghost"
      onClick={onClick}
      className={cn(
        `h-9 shrink-0 gap-1.5 rounded-none border-r border-border px-3 text-xs font-normal`,
        expanded && `bg-muted`,
        // EXP-550: the host machine is offline — the agent is parked, the
        // tab greys out instead of pinging "live".
        paused && `opacity-60`
      )}
      title={
        paused ? `Paused · ${device.label ?? `the device`} is offline` : undefined
      }
    >
      {paused ? (
        <span className="size-2 shrink-0 rounded-full bg-muted-foreground/40" />
      ) : session.status === `running` ||
        session.status === `in_review` ||
        session.status === `merged` ? (
        (() => {
          // EXP-214 display split: needs-input amber beats everything, a
          // merged PR/session renders blue, review stays green.
          const state = sessionDisplayState(session, issue?.prState)
          if (state === `running`) return <RunningDot />
          const dot =
            state === `needs_input`
              ? `bg-amber-500`
              : state === `done` || state === `merged`
                ? `bg-sky-500`
                : `bg-emerald-500`
          return <span className={`size-2 shrink-0 rounded-full ${dot}`} />
        })()
      ) : (
        <span className="size-2 shrink-0 rounded-full bg-muted-foreground/40" />
      )}
      <span className="max-w-[10rem] truncate font-mono">{label}</span>
      {device.label && (
        <span className="max-w-[8rem] truncate text-muted-foreground">
          {` · ${device.label}`}
        </span>
      )}
    </Button>
  )
}
