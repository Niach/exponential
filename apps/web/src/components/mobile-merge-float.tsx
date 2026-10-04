import {
  MOBILE_MERGE_FLOAT_CLEARANCE,
  MOBILE_WORK_BAR_CLEARANCE,
} from "@exp/ui"
import { MergeCapsule } from "@/components/issue-changes-face"

// EXP-1154: the phone's ONE merge on the faces that keep a composer bar
// (Issue: Properties · Comment · Start; Run: ring · composer · Start). The
// white Merge capsule floats centred directly ABOVE that bar, 10px clear of
// it (52px bar + its safe-area padding + the gap), above it in z. It hides
// while the composer is expanded over the keyboard or the bar is hidden, and
// self-hides unless the PR is open. The Changes and Results faces put the
// same `MergeCapsule` IN their bar's cluster instead.

export const MOBILE_MERGE_FLOAT_BOTTOM = `calc(52px + max(1rem, env(safe-area-inset-bottom)) + 10px)`

/** The bottom clearance a face scroller under the bar reserves: the bar's,
 *  plus the float's 52px + 10px gap while the float is mounted. */
export function mobileFaceClearance(floats: boolean): string {
  return floats ? MOBILE_MERGE_FLOAT_CLEARANCE : MOBILE_WORK_BAR_CLEARANCE
}

export function MobileMergeFloat({
  hidden = false,
  ...target
}: {
  /** The composer expanded or the bar hidden. */
  hidden?: boolean
  issueId?: string
  sessionId?: string
  prState: string | null
  prNumber: number | null
  branch: string | null
  updatedAt: string | Date | null
  steerEnabled: boolean
}) {
  if (hidden || target.prState !== `open`) return null
  return (
    <div
      data-testid="mobile-merge-float"
      className="pointer-events-none fixed inset-x-0 z-[36] flex justify-center px-5 md:hidden"
      style={{ bottom: MOBILE_MERGE_FLOAT_BOTTOM }}
    >
      <MergeCapsule {...target} />
    </div>
  )
}
