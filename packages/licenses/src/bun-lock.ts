// EXP-375 / EXP-1205 — bun.lock as a dependency graph.
//
// `bun.lock` is committed, so anything derived from it alone reads the same on
// every host — including the pure-bun `web` CI job, which has no node_modules
// of its own to walk. Two consumers share this file:
//
//   - `scripts/collect-npm.ts` resolves platform-gated packages here (the
//     other host's prebuilt variant is never on disk) and takes the walk order
//     of their children from the lock.
//   - `apps/web/src/lib/licenses.test.ts` recomputes each scope's WHOLE
//     production closure from the lock and byte-compares its `name@version`
//     set with the committed inventory. That is the drift gate: the collector
//     needs node_modules for licence bodies, but the SET of components is a
//     pure function of the lockfile, so the gate can prove it without a
//     toolchain. EXP-1205 is what happens without this: a dependency added to
//     a workspace package (`@exp/ui`) reaches both apps only transitively, and
//     a gate that checks the apps' direct dependencies never notices.
//
// The lookup mirrors node resolution inside bun's flattened key space: a
// nested copy lives under `<importer key>/<name>` (`cheerio/parse5`), the
// hoisted one under the bare name. Workspace importers are keyed by their
// package NAME (`@exp/widget/typescript`), not their directory.

import { byCodepoint } from "./schema"

/** bun.lock is JSONC — trailing commas, no comments. Strip them outside
 *  strings rather than pull in a parser. */
export const parseJsonc = (text: string): unknown => {
  let out = ``
  let inString = false
  let escaped = false
  for (let i = 0; i < text.length; i++) {
    const c = text[i]!
    if (inString) {
      out += c
      if (escaped) escaped = false
      else if (c === `\\`) escaped = true
      else if (c === `"`) inString = false
      continue
    }
    if (c === `"`) {
      inString = true
      out += c
      continue
    }
    if (c === `,`) {
      let j = i + 1
      while (j < text.length && /\s/.test(text[j]!)) j++
      if (text[j] === `}` || text[j] === `]`) continue
    }
    out += c
  }
  return JSON.parse(out)
}

export interface LockWorkspace {
  /** Directory relative to the repo root (`apps/web`); `` is the root. */
  dir: string
  name: string
  dependencies: Record<string, string>
  optionalDependencies: Record<string, string>
}

export interface LockEntry {
  /** The flattened key the entry lives under (`cheerio/parse5`). */
  key: string
  name: string
  version: string
  /** `workspace:<dir>` versions are our own packages. */
  isWorkspace: boolean
  dependencies: Record<string, string>
  optionalDependencies: Record<string, string>
  os?: unknown
  cpu?: unknown
}

export interface BunLock {
  /** Keyed by directory. */
  workspaces: Map<string, LockWorkspace>
  /** Keyed by flattened lock key. */
  packages: Map<string, LockEntry>
}

const record = (value: unknown): Record<string, string> =>
  typeof value === `object` && value !== null
    ? (value as Record<string, string>)
    : {}

export const parseBunLock = (text: string): BunLock => {
  const raw = parseJsonc(text) as {
    workspaces: Record<string, Record<string, unknown>>
    packages: Record<string, unknown[]>
  }
  const workspaces = new Map<string, LockWorkspace>()
  for (const [dir, ws] of Object.entries(raw.workspaces)) {
    workspaces.set(dir, {
      dir,
      name: typeof ws.name === `string` ? ws.name : ``,
      dependencies: record(ws.dependencies),
      optionalDependencies: record(ws.optionalDependencies),
    })
  }
  const packages = new Map<string, LockEntry>()
  for (const [key, value] of Object.entries(raw.packages)) {
    const ident = String(value[0] ?? key)
    const at = ident.lastIndexOf(`@`)
    const name = at > 0 ? ident.slice(0, at) : ident
    const version = at > 0 ? ident.slice(at + 1) : ``
    const meta = (
      typeof value[2] === `object` && value[2] !== null ? value[2] : {}
    ) as Record<string, unknown>
    packages.set(key, {
      key,
      name,
      version,
      isWorkspace: version.startsWith(`workspace:`),
      dependencies: record(meta.dependencies),
      optionalDependencies: record(meta.optionalDependencies),
      os: meta.os,
      cpu: meta.cpu,
    })
  }
  return { workspaces, packages }
}

/** Split a bun.lock key into segments, honouring `@scope/name` segments. */
export const splitLockKey = (key: string): string[] => {
  if (key === ``) return []
  const parts = key.split(`/`)
  const out: string[] = []
  for (let i = 0; i < parts.length; i++) {
    if (parts[i]!.startsWith(`@`) && i + 1 < parts.length)
      out.push(`${parts[i]}/${parts[++i]}`)
    else out.push(parts[i]!)
  }
  return out
}

/** Node-style lookup inside bun.lock's flattened key space. */
export const lockLookup = (
  lock: BunLock,
  importerKey: string | null,
  name: string
): LockEntry | undefined => {
  const chain = splitLockKey(importerKey ?? ``)
  for (let i = chain.length; i >= 0; i--) {
    const candidate = [...chain.slice(0, i), name].join(`/`)
    const hit = lock.packages.get(candidate)
    if (hit) return hit
  }
  return undefined
}

/**
 * The PRODUCTION closure of one workspace, from the lockfile alone: its
 * `dependencies` + `optionalDependencies` (never `devDependencies`), walked
 * transitively through each resolved entry's own two fields. Our `@exp/*`
 * workspaces are traversed THROUGH and never returned — the same rules as
 * `collect-npm.ts`, whose output this must equal `name@version` for
 * `name@version`. Returned sorted by codepoint.
 */
export const productionClosure = (
  lock: BunLock,
  workspaceDir: string
): { name: string; version: string }[] => {
  const root = lock.workspaces.get(workspaceDir)
  if (!root) throw new Error(`bun.lock has no workspace at ${workspaceDir}`)

  const found = new Map<string, { name: string; version: string }>()
  const visited = new Set<string>()
  const queue: { importerKey: string; deps: string[] }[] = []

  const enqueueWorkspace = (ws: LockWorkspace) => {
    if (visited.has(`workspace:${ws.dir}`)) return
    visited.add(`workspace:${ws.dir}`)
    queue.push({
      importerKey: ws.name,
      deps: [
        ...Object.keys(ws.dependencies),
        ...Object.keys(ws.optionalDependencies),
      ].sort(byCodepoint),
    })
  }
  enqueueWorkspace(root)

  while (queue.length > 0) {
    const task = queue.shift()!
    for (const name of task.deps) {
      const entry = lockLookup(lock, task.importerKey, name)
      if (!entry) continue // not part of the installed graph
      if (entry.isWorkspace) {
        const dir = entry.version.slice(`workspace:`.length)
        const ws = lock.workspaces.get(dir)
        if (ws) enqueueWorkspace(ws)
        continue
      }
      const id = `${entry.name}@${entry.version}`
      if (!found.has(id)) found.set(id, { name: entry.name, version: entry.version })
      if (visited.has(entry.key)) continue
      visited.add(entry.key)
      queue.push({
        importerKey: entry.key,
        deps: [
          ...Object.keys(entry.dependencies),
          ...Object.keys(entry.optionalDependencies),
        ].sort(byCodepoint),
      })
    }
  }

  return [...found.values()].sort(
    (a, b) =>
      byCodepoint(a.name, b.name) || byCodepoint(a.version, b.version)
  )
}
