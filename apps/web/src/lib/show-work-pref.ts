import { useSyncExternalStore } from "react"
import { safeLocalStorage } from "@/lib/local-storage"
import { SHOW_WORK_DEFAULT } from "@/lib/work-faces"

// EXP-1175: the Run face's Show work preference — per USER, per browser. One
// localStorage key per user id (`exp.showWork:<userId>`), never synced. A
// module store like `sidebar-widths.ts`, so every mounted Run face flips
// together. Anything but a stored `true`/`false` reads as the default.

export function showWorkStorageKey(userId: string): string {
  return `exp.showWork:${userId}`
}

export function parseShowWork(raw: string | null | undefined): boolean {
  if (raw === `true`) return true
  if (raw === `false`) return false
  return SHOW_WORK_DEFAULT
}

const cache = new Map<string, boolean>()
const listeners = new Set<() => void>()

function read(userId: string): boolean {
  const hit = cache.get(userId)
  if (hit !== undefined) return hit
  let raw: string | null = null
  try {
    raw = safeLocalStorage()?.getItem(showWorkStorageKey(userId)) ?? null
  } catch {
    raw = null
  }
  const value = parseShowWork(raw)
  cache.set(userId, value)
  return value
}

export function setShowWork(userId: string, on: boolean): void {
  cache.set(userId, on)
  try {
    safeLocalStorage()?.setItem(showWorkStorageKey(userId), on ? `true` : `false`)
  } catch {
    // Blocked storage: the choice still holds for this page's life.
  }
  for (const listener of listeners) listener()
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

export function useShowWork(userId: string): boolean {
  return useSyncExternalStore(
    subscribe,
    () => read(userId),
    () => SHOW_WORK_DEFAULT
  )
}

/** Tests only: forget the in-memory values so the next read re-parses storage. */
export function resetShowWorkStoreForTests(): void {
  cache.clear()
}
