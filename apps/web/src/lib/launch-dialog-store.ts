import { useSyncExternalStore } from "react"

import type { DetailOrigin } from "@/lib/detail-origin"
import type { LaunchSeed } from "@/lib/launch-seed"

// EXP-1019: the start-coding DIALOG's open state, and nothing else.
//
// Every play button in the app used to navigate to the Agent page with a
// preselection in the URL, which put the person in front of a composer whose
// subject was one small badge — nobody read that as "your action is loaded,
// press send". A subject-carrying start opens the composer as a DIALOG over
// wherever the click happened instead, and the dialog says what it is about
// in its headline.
//
// The open state is deliberately NOT in the URL (a launcher is not a
// destination; a shared link must not reopen someone else's half-typed run)
// and deliberately NOT React state — the callers are scattered across the
// whole team subtree (issue detail, the bulk bar, the actions page, the
// sidebar's pinned rows, Reviews) with the layout between them. So it is the
// three-line external store the agent-login dialog and the Recent panel
// already use, with ONE `LaunchDialogHost` mounted in the team route.

/** What a play button hands the launcher: the seed, plus (EXP-870) the
 *  ORIGIN the click came from when the caller knows better than the URL —
 *  `null` from a pinned row means context-free, so the run it starts opens
 *  full-width with no list nav. The KEY absent = derive it from the URL,
 *  the same present-or-absent rule `useOpenSession` reads. */
export interface LaunchDialogRequest extends LaunchSeed {
  origin?: DetailOrigin | null
}

let pending: LaunchDialogRequest | null = null
const listeners = new Set<() => void>()

function emit(): void {
  for (const listener of listeners) listener()
}

/** Open the launcher on `seed`. A second call re-seeds the open dialog. */
export function requestLaunchDialog(seed: LaunchDialogRequest): void {
  pending = seed
  emit()
}

/** Shut it — on dismissal, and once a run has been started. */
export function closeLaunchDialog(): void {
  if (pending === null) return
  pending = null
  emit()
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

/** The seed the dialog is open on, or `null` while it is shut. */
export function useLaunchDialogSeed(): LaunchDialogRequest | null {
  // Server render: the launcher is always shut.
  return useSyncExternalStore(
    subscribe,
    () => pending,
    () => null
  )
}
