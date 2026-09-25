import { GlassSectionHeader } from "@exp/ui"
import { SessionTree } from "@/components/session-tree"
import { usePastRuns } from "@/hooks/use-agents-data"
import { useOpenSession } from "@/hooks/use-open-session"

// EXP-923: the Agent page's RECENT runs, as a sidebar panel.
//
// The Agent page is the composer and nothing else now — its history lives
// behind ONE ghost history button at the page's top-left, which slides this
// panel into the sidebar's panel slot beside the compact rail (the phone gets
// the same list in a bottom sheet from the topbar instead). Plain list, no
// fold: the band is a label, not a control — the toggle IS the disclosure.
//
// A row opens its run exactly as before, with the `agent` origin, so Back
// returns to the Agent page and a tab is created like any other opened work.

export function RecentRunsSidebar({
  teamId,
  currentUserId,
}: {
  teamId: string
  currentUserId: string | undefined
}) {
  const { past } = usePastRuns(teamId, currentUserId)
  const openSession = useOpenSession()
  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-y-auto p-2">
      <GlassSectionHeader label="Recent" />
      <SessionTree
        rows={past}
        onOpen={(session) => openSession(session, { origin: { kind: `agent` } })}
        emptyNote="Nothing has finished yet."
      />
    </div>
  )
}

/** The same rows for the phone's bottom sheet (`mobile-topbar.tsx`). */
export function RecentRunsList({
  teamId,
  currentUserId,
  onOpened,
}: {
  teamId: string
  currentUserId: string | undefined
  onOpened: () => void
}) {
  const { past } = usePastRuns(teamId, currentUserId)
  const openSession = useOpenSession()
  return (
    <SessionTree
      rows={past}
      onOpen={(session) => {
        onOpened()
        openSession(session, { origin: { kind: `agent` } })
      }}
      emptyNote="Nothing has finished yet."
    />
  )
}
