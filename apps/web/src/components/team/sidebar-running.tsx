import type * as React from "react"
import { useNavigate, useParams, useRouterState } from "@tanstack/react-router"
import {
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
} from "@exp/ui"
import { ThreadLead, ThreadRow } from "@/components/thread/thread-row"
import { useThreadList, type ThreadBundle } from "@/hooks/use-threads"
import { TABLESS_ORIGIN } from "@/lib/work-tabs"

// EXP-923: RUNNING lives in the SIDEBAR — hidden entirely when nothing runs
// (no empty copy, the desktop's `RunningSessionsSection` rule). A click
// navigates with the `running` origin, which creates NO work tab
// (`lib/work-tabs.ts` `TABLESS_ORIGIN`) and keeps the main menu up.
//
// EXP-1284: lists show THREADS, not bare runs. A row is a thread holding a
// live run of mine (`partitionThreadList` → running): its title over the
// model's `listCaption` (`2 issues · 1 run · macbook`), led by the live
// run's mark (the spark while it works, the amber badge while it waits on a
// person), opening the thread page.

/** The threads the section lists — shared by the expanded group and the
 *  compact rail's icon column. */
export function useMyRunningThreads(
  teamId: string | undefined,
  currentUserId: string | undefined
): ThreadBundle[] {
  return useThreadList(teamId, currentUserId).running
}

/** Which thread the app is SHOWING right now: its page, or the page of one
 *  of its runs / issues. */
function useShownThread(): (bundle: ThreadBundle) => boolean {
  const pathname = useRouterState({ select: (s) => s.location.pathname })
  const { issueIdentifier } = useParams({ strict: false }) as {
    issueIdentifier?: string
  }
  const threadId = /\/threads\/([^/]+)$/.exec(pathname)?.[1] ?? null
  const runId = /\/sessions\/([^/]+)$/.exec(pathname)?.[1] ?? null
  return (bundle) => {
    if (threadId) return bundle.thread.id === threadId
    if (runId) return bundle.runs.some((run) => run.id === runId)
    if (issueIdentifier) {
      return bundle.issues.some((issue) => issue.identifier === issueIdentifier)
    }
    return false
  }
}

function useOpenThread(): (bundle: ThreadBundle) => void {
  const navigate = useNavigate()
  const { teamSlug } = useParams({ strict: false }) as { teamSlug?: string }
  return (bundle) => {
    if (!teamSlug) return
    void navigate({
      to: `/t/$teamSlug/threads/$threadId`,
      params: { teamSlug, threadId: bundle.thread.id },
      search: { from: TABLESS_ORIGIN },
    })
  }
}

/** EXP-923: the expanded sidebar's Running group — hidden with nothing live. */
export function SidebarRunningSection({
  teamId,
  currentUserId,
}: {
  teamId: string | undefined
  currentUserId: string | undefined
}) {
  const threads = useMyRunningThreads(teamId, currentUserId)
  const isShown = useShownThread()
  const openThread = useOpenThread()
  if (threads.length === 0) return null
  return (
    <SidebarGroup data-testid="sidebar-running">
      {/* EXP-1022: the main menu's plain group label, like Pinned and Boards
          above it — the filled group band belongs to LISTS, not the menu. */}
      <SidebarGroupLabel>Running</SidebarGroupLabel>
      <SidebarGroupContent>
        <div className="flex flex-col">
          {threads.map((bundle) => (
            <ThreadRow
              key={bundle.thread.id}
              bundle={bundle}
              active={isShown(bundle)}
              ringClassName="ring-sidebar"
              onOpen={() => openThread(bundle)}
              testId={`sidebar-running-${bundle.thread.id}`}
            />
          ))}
        </div>
      </SidebarGroupContent>
    </SidebarGroup>
  )
}

/** EXP-923: the compact rail's Running column — one 32px button per running
 *  thread under the board icons. The rail owns the button chrome, so it
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
  const threads = useMyRunningThreads(teamId, currentUserId)
  const isShown = useShownThread()
  const openThread = useOpenThread()
  if (threads.length === 0) return null
  return (
    <>
      {separator}
      {threads.map((bundle) =>
        renderItem({
          key: bundle.thread.id,
          label: bundle.view.title,
          active: isShown(bundle),
          onClick: () => openThread(bundle),
          icon: <ThreadLead bundle={bundle} />,
        })
      )}
    </>
  )
}
