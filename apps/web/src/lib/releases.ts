import type { IssueStatus } from "@/lib/domain"
import { timestampMs } from "@/lib/project-board"

// Release progress + ordering (EXP-56 §10.2/§10.3) — the cross-platform
// contract, mirrored on iOS, Android, and desktop. Pure functions shared by
// the releases list, the release detail, and the sidebar badge.

// The minimal shape progress counting needs — `Issue` satisfies it.
export interface ProgressCountableIssue {
  status: IssueStatus
}

export interface ReleaseProgress {
  total: number
  done: number
  // cancelled/duplicate issues are *dropped*, not shipped — they leave the
  // denominator instead of counting as progress.
  dropped: number
  denominator: number
  fraction: number
  // "Ready to ship": every non-dropped issue is done. Independent of
  // shipped_at — shipping early (or an empty release) is allowed, it just
  // never reads as Ready.
  isComplete: boolean
}

export function releaseProgress(
  issues: ProgressCountableIssue[]
): ReleaseProgress {
  const total = issues.length
  let done = 0
  let dropped = 0
  for (const issue of issues) {
    if (issue.status === `done`) {
      done += 1
    } else if (issue.status === `cancelled` || issue.status === `duplicate`) {
      dropped += 1
    }
  }
  const denominator = total - dropped
  return {
    total,
    done,
    dropped,
    denominator,
    fraction: denominator > 0 ? done / denominator : 0,
    isComplete: denominator > 0 && done === denominator,
  }
}

// The minimal shape the comparator needs — `Release` satisfies it, and so do
// optimistic upserts whose timestamps arrive as strings (EXP-38 gotcha,
// handled by `timestampMs`).
export interface SortableRelease {
  targetDate: string | null
  shippedAt: Date | string | null
  createdAt: Date | string
}

// Canonical release ordering: unshipped before shipped; unshipped by
// targetDate asc with nulls LAST (a dated release is more urgent than an
// undated one) then createdAt desc; shipped by shippedAt desc (most recently
// shipped first).
export function compareReleases(
  a: SortableRelease,
  b: SortableRelease
): number {
  const aShipped = a.shippedAt !== null
  const bShipped = b.shippedAt !== null
  if (aShipped !== bShipped) {
    return aShipped ? 1 : -1
  }

  if (aShipped && bShipped) {
    return timestampMs(b.shippedAt!) - timestampMs(a.shippedAt!)
  }

  if (
    a.targetDate !== null &&
    b.targetDate !== null &&
    a.targetDate !== b.targetDate
  ) {
    // Safe as a string compare — `targetDate` is a plain DATE column.
    return a.targetDate < b.targetDate ? -1 : 1
  }
  if (a.targetDate !== null && b.targetDate === null) return -1
  if (a.targetDate === null && b.targetDate !== null) return 1

  return timestampMs(b.createdAt) - timestampMs(a.createdAt)
}
