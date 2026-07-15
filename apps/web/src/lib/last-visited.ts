// Last-visited workspace/project persistence (EXP-69). Per-device via
// localStorage: the workspace layout writes an entry on every workspace or
// project navigation, and the root redirect (`routes/index.tsx`) reads it so
// app entry jumps back to where the user left off instead of `/t/default`.
//
// The app is fully client-rendered (`defaultSsr: false`), but route code can
// still run where `window` is missing and localStorage access can throw
// (privacy modes, blocked storage) — every accessor guards and degrades to
// "no persistence" instead of breaking navigation.

export interface LastVisited {
  workspaceSlug: string
  projectSlug?: string
}

const STORAGE_KEY = `exp.lastVisited`

function storage(): Storage | null {
  if (typeof window === `undefined`) return null
  try {
    return window.localStorage
  } catch {
    return null
  }
}

export function readLastVisited(): LastVisited | null {
  const store = storage()
  if (!store) return null
  try {
    const raw = store.getItem(STORAGE_KEY)
    if (!raw) return null
    const parsed: unknown = JSON.parse(raw)
    if (typeof parsed !== `object` || parsed === null) return null
    const record = parsed as Record<string, unknown>
    if (
      typeof record.workspaceSlug !== `string` ||
      record.workspaceSlug.length === 0
    ) {
      return null
    }
    return {
      workspaceSlug: record.workspaceSlug,
      projectSlug:
        typeof record.projectSlug === `string` && record.projectSlug.length > 0
          ? record.projectSlug
          : undefined,
    }
  } catch {
    return null
  }
}

// Remember a navigation. A workspace-level visit (no `projectSlug` — Inbox,
// My Issues, settings, …) keeps the stored project while the workspace is
// unchanged, so "last-used project" survives a detour; it drops the project
// when the workspace changed (a project slug is only meaningful inside its
// own workspace).
export function rememberLastVisited(
  workspaceSlug: string,
  projectSlug?: string
): void {
  const store = storage()
  if (!store) return
  const previous = readLastVisited()
  const next: LastVisited = {
    workspaceSlug,
    projectSlug:
      projectSlug ??
      (previous?.workspaceSlug === workspaceSlug
        ? previous.projectSlug
        : undefined),
  }
  try {
    store.setItem(STORAGE_KEY, JSON.stringify(next))
  } catch {
    // Quota/privacy failures just mean no persistence — never break nav.
  }
}

// Drop a stale entry (workspace deleted or membership lost) so the next app
// entry falls straight through to the `/t/default` resolution.
export function clearLastVisited(): void {
  const store = storage()
  if (!store) return
  try {
    store.removeItem(STORAGE_KEY)
  } catch {
    // Ignore — a stale entry that cannot be cleared is re-detected next time.
  }
}
