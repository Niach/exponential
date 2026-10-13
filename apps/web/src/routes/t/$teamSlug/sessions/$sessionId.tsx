import { useState } from "react"
import { createFileRoute, Link, redirect } from "@tanstack/react-router"
import {
  Button,
  conceptIcon,
  Pill,
  SkeletonRows,
  toast,
  WORK_COLUMN_CLASS,
  WORK_GUTTER_CLASS,
} from "@exp/ui"
import type { IconConcept } from "@exp/icons"
import { AgentSessionView } from "@/components/agent-session"
import { SessionComposer } from "@/components/session-composer"
import { Empty } from "@/components/empty"
import { emptyStateCopy } from "@/lib/empty-states"
import {
  useSessionCard,
  useSessionRow,
  useSessionThread,
  useTreeSessions,
} from "@/hooks/use-session-tree"
import { useTeamBySlug, useTeamUsers } from "@/hooks/use-team-data"
import { pageTitle, usePageTitle } from "@/lib/page-title"
import type { SessionRow } from "@/lib/session-tree"
import { trpc } from "@/lib/trpc-client"
import { cn } from "@/lib/utils"

const BackIcon = conceptIcon(`ui-back`)
const StopIcon = conceptIcon(`coding-stop`)

// EXP-1293 Threads v2: ONE session's RAW WORK, opened from its session card
// (a thread's conversation, an agent's channel). The header leads back to
// its thread (the root's title), names the session (the card's name, `Session
// N` untitled) and offers Stop while it is live; under it the feed always
// shows the work (no faces, no Show work, no Guide, no issue header), and the
// reply box (`SessionComposer`) posts to THIS session. Visibility is the
// shape's: a shared session syncs to every member, a private one to its
// starter alone, so whoever can see it may post, answer and Stop.

export const Route = createFileRoute(`/t/$teamSlug/sessions/$sessionId`)({
  head: () => ({ meta: [{ title: pageTitle(`Session`) }] }),
  beforeLoad: async ({ context, location }) => {
    if (!context.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: location.href },
      })
    }
  },
  component: SessionPage,
})

function SessionPage() {
  const { teamSlug, sessionId } = Route.useParams()
  const team = useTeamBySlug(teamSlug)
  const { row, isReady } = useSessionRow(sessionId)

  if (!team || (!row && !isReady)) {
    return (
      <div className="flex h-full min-h-0 flex-col">
        {/* D8 first load: the detail page's placeholder (3 list rows). */}
        <div className="p-6">
          <SkeletonRows />
        </div>
      </div>
    )
  }

  if (!row) {
    return (
      <div className="flex h-full min-h-0 flex-col items-center justify-center">
        <Empty id="run-not-found">
          <Button asChild variant="outline" size="sm">
            <Link to="/t/$teamSlug/agent" params={{ teamSlug }}>
              {emptyStateCopy(`run-not-found`).action}
            </Link>
          </Button>
        </Empty>
      </div>
    )
  }

  return <SessionScreen key={row.id} teamSlug={teamSlug} row={row} />
}

/** The loaded session: a child so its hooks stay unconditional. */
function SessionScreen({ teamSlug, row }: { teamSlug: string; row: SessionRow }) {
  const rootId = row.rootId ?? row.id
  const { tree } = useTreeSessions(rootId)
  const card = useSessionCard(row, tree)
  const { view } = useSessionThread(rootId)
  const { users } = useTeamUsers(row.teamId)
  const threadTitle = view?.title ?? null
  const name = card?.name ?? row.title ?? `Session`
  usePageTitle(pageTitle(name, threadTitle ?? `Session`))

  const live = row.status !== `ended`
  const isRoot = rootId === row.id
  const CardIcon = conceptIcon((card?.icon ?? `coding-assistant`) as IconConcept)

  const header = (
    <div
      className={cn(WORK_COLUMN_CLASS, WORK_GUTTER_CLASS, `shrink-0 pt-4 pb-2`)}
      data-testid="session-header"
    >
      {!isRoot || threadTitle ? (
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
        {live ? <StopPill sessionId={row.id} /> : null}
      </div>
    </div>
  )

  return (
    <AgentSessionView
      session={row}
      header={header}
      composer={
        <SessionComposer
          teamId={row.teamId}
          sessionId={row.id}
          placeholder="Reply…"
          users={users}
          busy={row.busy}
          canStop={live}
          testId="session-composer"
        />
      }
    />
  )
}

/** Stop the session (`sessions.stop`); the synced row ends it. */
function StopPill({ sessionId }: { sessionId: string }) {
  const [stopping, setStopping] = useState(false)
  return (
    <Pill
      mode="action"
      className="shrink-0 text-destructive"
      disabled={stopping}
      onClick={async () => {
        setStopping(true)
        try {
          await trpc.sessions.stop.mutate({ sessionId })
        } catch {
          toast.error(`Couldn't stop the session`)
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
