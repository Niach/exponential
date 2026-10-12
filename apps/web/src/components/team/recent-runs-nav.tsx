import { useParams } from "@tanstack/react-router"
import { ThreadListSection, useOpenThread } from "@/components/thread/thread-list"
import { useThreadList, type ThreadBundle } from "@/hooks/use-threads"
import type { Team } from "@/db/schema"
import { groupByTeam, TeamBandHeader } from "@/components/team/team-band"

// EXP-923: the Agent page's RECENT list. The page is the composer alone; its
// history sits behind ONE ghost history button that slides this list into the
// sidebar's panel slot (the phone gets the same list in a bottom sheet from
// the topbar instead).
//
// EXP-1246: Recent is a LIST-DETAIL list like the Inbox — the list-detail host
// (`list-detail.tsx`) renders it and a row opens with the `agent:recent`
// origin, so the list STAYS beside what it opened (the open one highlighted)
// and Back lands on the Agent page with it still open.
//
// EXP-1284: lists show THREADS, not bare runs: Recent = every thread that
// holds no live run of mine (`partitionThreadList`), newest activity first,
// each row the thread's title over its list caption, opening the thread.

/** The md+ panel body — the host owns the back row. */
export function RecentRunsNav({
  teamId,
  currentUserId,
}: {
  teamId: string
  currentUserId: string | undefined
}) {
  const { recent, isReady } = useThreadList(teamId, currentUserId)
  const openThread = useOpenThread()
  const { threadId } = useParams({ strict: false }) as { threadId?: string }
  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-y-auto p-2">
      <ThreadListSection
        label="Recent"
        threads={recent}
        loading={!isReady}
        activeThreadId={threadId ?? null}
        ringClassName="ring-sidebar"
        onOpen={(bundle) =>
          openThread(bundle, { origin: { kind: `agent`, tab: `recent` } })
        }
        empty="recent-runs"
      />
    </div>
  )
}

/** The same rows for the phone's bottom sheet (`mobile-topbar.tsx`).
 *  EXP-1186: across every member team (`teams`), one band per team while
 *  `grouped`; a row opens under ITS team's slug. */
export function RecentRunsList({
  teamId,
  teams,
  grouped = false,
  currentUserId,
  onOpened,
}: {
  teamId: string
  teams?: readonly Team[]
  grouped?: boolean
  currentUserId: string | undefined
  onOpened: () => void
}) {
  const scopeIds = teams && teams.length > 0 ? teams.map((t) => t.id) : teamId
  const { recent, isReady } = useThreadList(scopeIds, currentUserId)
  const openThread = useOpenThread()
  const open = (bundle: ThreadBundle) => {
    onOpened()
    const teamSlug = teams?.find((team) => team.id === bundle.thread.teamId)?.slug
    openThread(bundle, {
      origin: { kind: `agent` },
      ...(teamSlug ? { teamSlug } : {}),
    })
  }
  const groups =
    grouped && teams ? groupByTeam(teams, recent, (bundle) => bundle.thread.teamId) : []
  if (groups.length > 0) {
    return (
      <>
        {groups.map((group) => (
          <div key={group.team.id} className="mb-4 last:mb-0">
            <TeamBandHeader team={group.team} />
            <ThreadListSection threads={group.rows} onOpen={open} />
          </div>
        ))}
      </>
    )
  }
  return (
    <ThreadListSection threads={recent} loading={!isReady} onOpen={open} empty="recent-runs" />
  )
}
