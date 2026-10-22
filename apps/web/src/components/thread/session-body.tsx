import { useState } from "react"
import { Link } from "@tanstack/react-router"
import {
  conceptIcon,
  FloatingComposer,
  Pill,
  useIsMobile,
  WORK_COLUMN_CLASS,
  WORK_GUTTER_CLASS,
} from "@exp/ui"
import type { IconConcept } from "@exp/icons"
import { AgentSessionView } from "@/components/agent-session"
import { SessionComposer, SessionComposerReplies } from "@/components/session-composer"
import { stopSessionTurn } from "@/lib/session-stop"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import {
  useSessionCard,
  useSessionRow,
  useSessionThread,
} from "@/hooks/use-session-tree"
import { useAgentRow } from "@/hooks/use-agent-row"
import { useMarkRead, useNewestAgentMessageAt } from "@/hooks/use-mark-read"
import { useTeamUsers } from "@/hooks/use-team-data"
import { pageTitle, usePageTitle } from "@/lib/page-title"
import type { SessionRow } from "@/lib/session-tree"
import { cn } from "@/lib/utils"

// EXP-1293 / EXP-1316: ONE session's RAW WORK. Two hosts:
//
//   InPlaceSession   the body of the thread page's session chip
//                    (`?tab=session&session=<id>`) under the thread's own
//                    header + strip; the header carries its Stop pill
//                    (EXP-1343: phones too, a page of the thread's pager)
//   SessionScreen    sessions under a team AGENT (`/sessions/$id`): its own header,
//                    back to the thread (or the agent's channel), the
//                    session's name and Stop while live
//
// The feed always shows the work (no faces, no Show work, no Guide), and the
// reply box posts to THIS session. Visibility is the shape's. EXP-1323:
// opening either marks THIS session read (`useOpenedSessionRead`).

const BackIcon = conceptIcon(`ui-back`)
const StopIcon = conceptIcon(`coding-stop`)
const REPLY_PLACEHOLDER = `Reply…`

/** The reply box under a session's feed. */
function SessionReply({ row, opaque = false }: { row: SessionRow; opaque?: boolean }) {
  const { users } = useTeamUsers(row.teamId)
  const live = row.status !== `ended`
  return (
    <SessionComposer
      teamId={row.teamId}
      sessionId={row.id}
      placeholder={REPLY_PLACEHOLDER}
      users={users}
      busy={row.busy}
      canStop={live}
      opaque={opaque}
      hideReplies={opaque}
      testId="session-composer"
    />
  )
}

/** EXP-1323: an open session view marks the session read, again whenever
 *  its newer agent reply lands while it stays open. */
function useOpenedSessionRead(sessionId: string) {
  useMarkRead(sessionId, useNewestAgentMessageAt([sessionId]))
}

/** The session open IN PLACE inside its thread: md+ under the thread's
 *  header + strip; EXP-1343 phones too — a PAGE of the thread's pager under
 *  its floating header band (`topInset` clears it), replying through THE
 *  floating composer capsule. */
export function InPlaceSession({
  row,
  topInset = 0,
  floating = false,
}: {
  row: SessionRow
  /** Phones: the overlay header band's height. */
  topInset?: number
  floating?: boolean
}) {
  useOpenedSessionRead(row.id)
  return (
    <div className="min-h-0 flex-1" data-testid="thread-session-body">
      <AgentSessionView
        key={row.id}
        session={row}
        header={topInset ? <div aria-hidden className="shrink-0" style={{ height: topInset }} /> : undefined}
        composerFloats={floating}
        composer={
          floating ? (
            <FloatingComposer
              placeholder={REPLY_PLACEHOLDER}
              above={<SessionComposerReplies sessionId={row.id} />}
            >
              <SessionReply row={row} opaque />
            </FloatingComposer>
          ) : (
            <SessionReply row={row} />
          )
        }
      />
    </div>
  )
}

/** The session on its own screen (phones; sessions under an agent). */
export function SessionScreen({ teamSlug, row }: { teamSlug: string; row: SessionRow }) {
  useOpenedSessionRead(row.id)
  const isMobile = useIsMobile()
  const rootId = row.rootId ?? row.id
  const isRoot = rootId === row.id
  const { row: syncedRoot } = useSessionRow(rootId === row.id ? null : rootId)
  const rootRow = isRoot ? row : syncedRoot
  // Under an agent's conversation (NEW-2: a root with `agentId`) the way
  // back is the agent's page, never /threads.
  const rootAgentId = rootRow?.agentId ?? null
  const { agent: rootAgent } = useAgentRow(rootAgentId)
  const card = useSessionCard(row)
  const { view } = useSessionThread(rootId)
  const threadTitle = view?.title ?? null
  // EXP-1341: the card's `sessionTitle`, else the thread's; never a generic
  // word (both hold while their shapes load).
  const name = card?.name ?? (isRoot ? threadTitle : null)
  usePageTitle(name ? pageTitle(name, isRoot ? null : threadTitle) : undefined)

  const live = row.status !== `ended`
  const CardIcon = conceptIcon((card?.icon ?? `coding-assistant`) as IconConcept)

  const header = (
    <div
      className={cn(WORK_COLUMN_CLASS, WORK_GUTTER_CLASS, `shrink-0 pt-4 pb-2`)}
      data-testid="session-header"
    >
      {rootAgentId ? (
        <Link
          to="/t/$teamSlug/agents/$agentId"
          params={{ teamSlug, agentId: rootAgentId }}
          className="inline-flex max-w-full min-w-0 items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground"
          data-testid="session-back"
        >
          <BackIcon className="size-4 shrink-0" />
          <span className="truncate">{rootAgent?.name || rootRow?.name || `Back`}</span>
        </Link>
      ) : !isRoot || threadTitle ? (
        <Link
          to="/t/$teamSlug/threads/$threadId"
          params={{ teamSlug, threadId: rootId }}
          className="inline-flex max-w-full min-w-0 items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground"
          data-testid="session-back"
        >
          <BackIcon className="size-4 shrink-0" />
          <span className="truncate">{threadTitle ?? `Back`}</span>
        </Link>
      ) : null}
      <div className="mt-3 flex min-w-0 items-center gap-3">
        <CardIcon className="size-5 shrink-0 text-muted-foreground" />
        <h1
          className="min-w-0 flex-1 truncate text-xl font-semibold"
          data-testid="session-title"
        >
          {name}
        </h1>
        {live ? <SessionStopPill sessionId={row.id} /> : null}
      </div>
    </div>
  )

  return (
    <AgentSessionView
      session={row}
      header={header}
      // EXP-1316: phones reply through THE floating composer capsule.
      composerFloats={isMobile}
      composer={
        isMobile ? (
          <FloatingComposer
              placeholder={REPLY_PLACEHOLDER}
              above={<SessionComposerReplies sessionId={row.id} />}
            >
            <SessionReply row={row} opaque />
          </FloatingComposer>
        ) : (
          <SessionReply row={row} />
        )
      }
    />
  )
}

/** Stop the session (`sessions.stop`); the synced row ends it. */
export function SessionStopPill({ sessionId }: { sessionId: string }) {
  const [stopping, setStopping] = useState(false)
  return (
    <Pill
      mode="action"
      className="shrink-0 text-destructive"
      disabled={stopping}
      onClick={async () => {
        setStopping(true)
        try {
          await stopSessionTurn(sessionId, SKIP_ERROR_TOAST)
        } catch (error) {
          failedAction(`stop`, `the session`, error)
        } finally {
          setStopping(false)
        }
      }}
      aria-label="Stop the session"
      title="Stop the session"
      data-testid="session-stop"
    >
      <StopIcon className="size-3.5" />
      Stop
    </Pill>
  )
}
