import { useState, type ReactNode } from "react"
import { Link } from "@tanstack/react-router"
import {
  Button,
  conceptIcon,
  FloatingComposer,
  Pill,
  SessionXref,
  toastActionError,
  useIsMobile,
  WORK_COLUMN_CLASS,
  WORK_GUTTER_CLASS,
} from "@exp/ui"
import type { IconConcept } from "@exp/icons"
import { AgentSessionView } from "@/components/agent-session"
import { SessionComposer, SessionComposerReplies } from "@/components/session-composer"
import { CHATS_COPY } from "@exp/domain-contract/chats"
import { actionErrorDescription, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { sessionCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import {
  useSessionCard,
  useSessionRow,
  useSessionThread,
} from "@/hooks/use-session-tree"
import { useAgentRow } from "@/hooks/use-agent-row"
import { useSessionNav, useXrefAgent } from "@/hooks/use-session-nav"
import type { SessionXref as SessionXrefModel } from "@exp/domain-contract/session-tree"
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
//                    session's name (Chats: no Stop pill under an agent)
//
// The feed always shows the work (no faces, no Show work, no Guide), and the
// reply box posts to THIS session. Visibility is the shape's. EXP-1323:
// opening either marks THIS session read (`useOpenedSessionRead`).

const BackIcon = conceptIcon(`ui-back`)
const PopIcon = conceptIcon(`ui-chevron-left`)

/** S-3: who started the session on show (`SessionThreadView.origin`) as
 *  THE xref line on top of its raw work. */
export function SessionOrigin({
  origin,
  onOpen,
  className,
}: {
  origin: SessionXrefModel | null | undefined
  onOpen?: (sessionId: string) => void
  className?: string
}) {
  const xrefAgent = useXrefAgent()
  if (!origin) return null
  return (
    <div className={cn(WORK_COLUMN_CLASS, WORK_GUTTER_CLASS, `flex shrink-0 pt-3 pb-1`, className)}>
      <SessionXref
        xref={origin}
        agent={origin.agentId ? xrefAgent(origin.agentId) : null}
        onOpen={onOpen}
      />
    </div>
  )
}

/** S-3: the header's `‹` popping the route's `back` stack (absent = none). */
export function BackStackButton({ onPop, className }: { onPop?: () => void; className?: string }) {
  if (!onPop) return null
  return (
    <Button
      variant="ghost"
      size="icon-sm"
      aria-label="Back"
      data-testid="thread-back"
      onClick={onPop}
      className={cn(`shrink-0 text-muted-foreground`, className)}
    >
      <PopIcon className="size-4" />
    </Button>
  )
}
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
  origin = null,
  onOpenXref,
}: {
  row: SessionRow
  /** Phones: the overlay header band's height. */
  topInset?: number
  floating?: boolean
  /** S-3: who started it (the thread view's `origin` with it in focus). */
  origin?: SessionXrefModel | null
  onOpenXref?: (sessionId: string) => void
}) {
  useOpenedSessionRead(row.id)
  let header: ReactNode = topInset ? <div aria-hidden className="shrink-0" style={{ height: topInset }} /> : undefined
  if (origin) {
    header = (
      <>
        {header}
        <SessionOrigin origin={origin} onOpen={onOpenXref} />
      </>
    )
  }
  return (
    <div className="min-h-0 flex-1" data-testid="thread-session-body">
      <AgentSessionView
        key={row.id}
        session={row}
        header={header}
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
export function SessionScreen({
  teamSlug,
  row,
  back,
}: {
  teamSlug: string
  row: SessionRow
  /** S-3: the route's `back` stack. */
  back?: string
}) {
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
  const { view } = useSessionThread(rootId, row.id)
  const nav = useSessionNav({ teamSlug, here: row.id, back })
  const threadTitle = view?.title ?? null
  // EXP-1341: the card's `sessionTitle`, else the thread's; never a generic
  // word (both hold while their shapes load).
  const name = card?.name ?? (isRoot ? threadTitle : null)
  usePageTitle(name ? pageTitle(name, isRoot ? null : threadTitle) : undefined)

  const CardIcon = conceptIcon((card?.icon ?? `coding-assistant`) as IconConcept)

  const header = (
    <div
      className={cn(WORK_COLUMN_CLASS, WORK_GUTTER_CLASS, `shrink-0 pt-4 pb-2`)}
      data-testid="session-header"
    >
      <div className="flex min-w-0 items-center gap-1">
      <BackStackButton onPop={nav.pop} className="-ml-1.5" />
      {rootAgentId ? (
        <Link
          to="/t/$teamSlug/agents/$agentId"
          params={{ teamSlug, agentId: rootAgentId }}
          className="inline-flex max-w-full min-w-0 items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground"
          data-testid="session-back"
        >
          <BackIcon className="size-4 shrink-0" />
          <span className="truncate">{rootAgent?.name || `Back`}</span>
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
      </div>
      <div className="mt-3 flex min-w-0 items-center gap-3">
        <CardIcon className="size-5 shrink-0 text-muted-foreground" />
        <h1
          className="min-w-0 flex-1 truncate text-xl font-semibold"
          data-testid="session-title"
        >
          {name}
        </h1>
        {/* Chats STOP: an agent's tree has no header pill; a thread's
            ends its root. */}
        {!rootAgentId && rootRow && rootRow.status !== `ended` ? (
          <SessionStopPill rootId={rootId} />
        ) : null}
      </div>
      {view?.origin ? <SessionOrigin origin={view.origin} onOpen={nav.open} className="px-0" /> : null}
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

/** Thread 58cb32df (chats.json STOP): the thread header's Stop pill = the
 *  chat menu's Stop, `sessions.end` on the ROOT (the whole tree ends); shown
 *  while the root is not ended. An agent chat has none (only the composer's
 *  turn stop and the agent menu's). A failure toasts `copy.toasts.stopFailed`. */
export function SessionStopPill({ rootId }: { rootId: string }) {
  const [stopping, setStopping] = useState(false)
  return (
    <Pill
      mode="action"
      className="shrink-0 text-destructive"
      disabled={stopping}
      onClick={async () => {
        setStopping(true)
        try {
          const result = await trpc.sessions.end.mutate({ sessionId: rootId }, SKIP_ERROR_TOAST)
          await sessionCollection.utils.awaitTxId(result.txid, 3000).catch(() => undefined)
        } catch (error) {
          toastActionError(CHATS_COPY.toasts.stopFailed, { description: actionErrorDescription(error) })
        } finally {
          setStopping(false)
        }
      }}
      aria-label={STOP_LABEL}
      title={STOP_LABEL}
      data-testid="session-stop"
    >
      <StopIcon className="size-3.5" />
      {CHATS_COPY.menu.stop}
    </Pill>
  )
}

const STOP_LABEL = `Stop the chat`
