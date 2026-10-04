import { SessionMergeButton } from "@/components/session-merge-button"

// EXP-1191: the phone's merge on the faces that keep a composer bar (Issue:
// Properties · Comment · Merge · Start; Run: ring · composer · Merge · Start)
// is a CIRCLE in the bar, right of the capsule — where the stack navigation
// circle used to sit — not the white capsule floating above it (EXP-1154),
// which outweighed the composer it hovered over. The bar's own `FabButton`,
// the merge glyph alone; it carries `SessionMergeButton`'s confirm, stack
// choice and Fix-conflicts swap and self-hides unless the PR is open. The
// Changes and Results faces keep the white `MergeCapsule` in their cluster.

export function MobileMergeCircle(target: {
  issueId?: string
  sessionId?: string
  prState: string | null
  prNumber: number | null
  branch: string | null
  updatedAt: string | Date | null
  steerEnabled: boolean
}) {
  return <SessionMergeButton {...target} as="fab" className="[&_svg]:size-5" />
}
