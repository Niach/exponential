// EXP-916: the review detail's FILE TREE lives in the sidebar's panel slot
// (`ReviewFilesNav`, beside the compact rail — the desktop's
// `LeftOccupant::ReviewFiles`), but the files are the page's: it fetches them
// (`useReviewFiles`) and owns which one the diff is scrolled to. The two
// render in different subtrees of the team layout, so the page PUBLISHES
// what the tree needs here and the sidebar subscribes. One slot per window:
// only one review is open at a time, and the page clears it on unmount so a
// stale tree never outlives its diff.
//
// A plain external store (`useSyncExternalStore`), not context: the sidebar
// must not re-render the whole team layout on every pick.
//
// EXP-945: the RUN's Changes face publishes the same slot. A run's diff is
// the same kind of context a review's is — its files, not the list it was
// opened from — so the two share this one channel and the one panel rather
// than the run growing a floating tree of its own inside the column. Only the
// back row differs, which is why a publisher may name its own.

import { useSyncExternalStore } from "react"
import type { DiffFile } from "@exp/domain-contract/diff"

export type ReviewFilesSlot = {
  /** The subject the files belong to (a review's issue id, a run's session
   *  id) — the tree re-keys on it. */
  subjectId: string
  /** `loading` until the fetch lands; `error` shows the message instead. */
  status: `loading` | `files` | `none` | `error`
  files: readonly DiffFile[]
  /** The file the diff is scrolled to. */
  selected: string | null
  /** A pick in the tree — the page scrolls its diff. */
  onSelect: (path: string) => void
  /** EXP-945: where the panel's back row goes. Absent = back to Reviews, the
   *  review detail's own row. */
  back?: { label: string; onBack: () => void }
}

let current: ReviewFilesSlot | null = null
const listeners = new Set<() => void>()

/** Publish the open review's tree inputs (or clear them with `null`). */
export function publishReviewFiles(slot: ReviewFilesSlot | null): void {
  if (current === slot) return
  current = slot
  for (const listener of listeners) listener()
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

function snapshot(): ReviewFilesSlot | null {
  return current
}

/** The published slot, live — `null` while no review is open. */
export function useReviewFilesSlot(): ReviewFilesSlot | null {
  return useSyncExternalStore(subscribe, snapshot, snapshot)
}

function subjectSnapshot(): string | null {
  return current?.subjectId ?? null
}

/** Only WHOSE files are published — a primitive snapshot, so a subscriber
 *  that merely asks "is a tree up for this subject?" (the team layout, via
 *  `useSidebarOccupant`) does not re-render on every diff tick or file pick
 *  the full slot republishes. */
export function useReviewFilesSubjectId(): string | null {
  return useSyncExternalStore(subscribe, subjectSnapshot, subjectSnapshot)
}
