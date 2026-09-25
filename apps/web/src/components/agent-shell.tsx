import { GlassSectionHeader } from "@exp/ui"
import { cn } from "@/lib/utils"
import { SessionTree } from "@/components/session-tree"
import { useAgentsData } from "@/hooks/use-agents-data"
import { useOpenSession } from "@/hooks/use-open-session"
import type { DetailOrigin } from "@/lib/detail-origin"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"

// EXP-818: the caller's sessions list — Running, nested by
// `parent_session_id` (the ×4 rule) with the EXP-965 connector.
//
// EXP-996: and the nesting is the whole `sessionTree` now — a workflow's runs
// and a stack's sit under one group row each, resumes collapsed.
//
// EXP-923: RUNNING ONLY. The Recent band is gone from here: on md+ the whole
// list left the Agent page for the sidebar (the Running section in the main
// menu, the Recent panel behind the page's history button), and on the phone
// Recent moved into the topbar's history sheet. What is left is the phone
// Agent page's own Running list under the composer.

/** The Running list — the Agent page's column on the phone. Every row hands
 *  the run it opens the `origin` this list stands for (EXP-851). */
export function SessionsList({
  teamId,
  currentUserId,
  activeSessionId,
  className,
  origin = null,
  scroll = true,
  showWhenEmpty = false,
}: {
  teamId: string
  currentUserId: string
  activeSessionId: string | null
  className?: string
  origin?: DetailOrigin | null
  /** EXP-851: the Agent page stacks the list UNDER the composer inside one
   *  scroller, so it turns this list's own scrollport off. */
  scroll?: boolean
  /** EXP-862: the Running band is ALWAYS drawn on the Agent page, empty or
   *  not — that page IS the composer plus what is running. */
  showWhenEmpty?: boolean
}) {
  const { running, isLoading } = useAgentsData(teamId, currentUserId)
  const openSession = useOpenSession()
  if (!showWhenEmpty && running.length === 0) return null
  return (
    <div
      className={cn(
        `p-2`,
        scroll && `min-h-0 flex-1 overflow-y-auto`,
        TAB_BAR_CLEARANCE,
        className
      )}
    >
      <GlassSectionHeader label="Running" />
      {isLoading ? (
        <div className="px-3 py-2 text-xs text-muted-foreground">Loading…</div>
      ) : (
        <SessionTree
          rows={running}
          teamId={teamId}
          activeSessionId={activeSessionId}
          onOpen={(session) => openSession(session, { origin })}
          emptyNote="No agents running right now."
        />
      )}
    </div>
  )
}
