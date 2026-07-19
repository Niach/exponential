// Last-visited team/board persistence (EXP-69). Per-device via
// localStorage: the team layout writes an entry on every team or
// board navigation, and the root redirect (`routes/index.tsx`) reads it so
// app entry jumps back to where the user left off instead of `/t/default`.
//
// Every accessor guards via `safeLocalStorage` and degrades to
// "no persistence" instead of breaking navigation.

import { safeLocalStorage } from "@/lib/local-storage"

export interface LastVisited {
  teamSlug: string
  boardSlug?: string
}

const STORAGE_KEY = `exp.lastVisited`

export function readLastVisited(): LastVisited | null {
  const store = safeLocalStorage()
  if (!store) return null
  try {
    const raw = store.getItem(STORAGE_KEY)
    if (!raw) return null
    const parsed: unknown = JSON.parse(raw)
    if (typeof parsed !== `object` || parsed === null) return null
    const record = parsed as Record<string, unknown>
    if (
      typeof record.teamSlug !== `string` ||
      record.teamSlug.length === 0
    ) {
      return null
    }
    return {
      teamSlug: record.teamSlug,
      boardSlug:
        typeof record.boardSlug === `string` && record.boardSlug.length > 0
          ? record.boardSlug
          : undefined,
    }
  } catch {
    return null
  }
}

// Remember a navigation. A team-level visit (no `boardSlug` — Inbox,
// My Issues, settings, …) keeps the stored board while the team is
// unchanged, so "last-used board" survives a detour; it drops the board
// when the team changed (a board slug is only meaningful inside its
// own team).
export function rememberLastVisited(
  teamSlug: string,
  boardSlug?: string
): void {
  const store = safeLocalStorage()
  if (!store) return
  const previous = readLastVisited()
  const next: LastVisited = {
    teamSlug,
    boardSlug:
      boardSlug ??
      (previous?.teamSlug === teamSlug
        ? previous.boardSlug
        : undefined),
  }
  try {
    store.setItem(STORAGE_KEY, JSON.stringify(next))
  } catch {
    // Quota/privacy failures just mean no persistence — never break nav.
  }
}

// Drop a stale entry (team deleted or membership lost) so the next app
// entry falls straight through to the `/t/default` resolution.
export function clearLastVisited(): void {
  const store = safeLocalStorage()
  if (!store) return
  try {
    store.removeItem(STORAGE_KEY)
  } catch {
    // Ignore — a stale entry that cannot be cleared is re-detected next time.
  }
}
