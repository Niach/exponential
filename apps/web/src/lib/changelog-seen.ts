// Per-device "What's new" dismissal (EXP-164). Stores the id of the newest
// changelog entry the user has seen/dismissed; the sidebar card shows only
// while the head of `CHANGELOG` differs from it. localStorage (not a server
// flag) is deliberate: the dismissal recurs per release, and re-showing once
// per device is normal what's-new UX.
//
// Every accessor guards via `safeLocalStorage` and degrades to
// "no persistence" instead of breaking the sidebar.

import { safeLocalStorage } from "@/lib/local-storage"

const STORAGE_KEY = `exp.changelogSeenId`

export function readSeenChangelogId(): string | null {
  const store = safeLocalStorage()
  if (!store) return null
  try {
    const raw = store.getItem(STORAGE_KEY)
    return raw && raw.length > 0 ? raw : null
  } catch {
    return null
  }
}

export function markChangelogSeen(id: string): void {
  const store = safeLocalStorage()
  if (!store) return
  try {
    store.setItem(STORAGE_KEY, id)
  } catch {
    // Quota/privacy failures just mean the card re-shows next visit.
  }
}
